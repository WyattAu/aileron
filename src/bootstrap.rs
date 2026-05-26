//! Application bootstrap and diagnostics utilities.

/// Install a panic hook that writes detailed crash info to a file.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Write crash report to file
        let crash_dir = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .map(|d| d.data_dir().join("crashes"))
            .unwrap_or_else(|| std::path::PathBuf::from("./crashes"));
        let _ = std::fs::create_dir_all(&crash_dir);

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let crash_path = crash_dir.join(format!("crash_{timestamp}.txt"));

        let report = format!(
            "=== Aileron Crash Report ===\n\
             Time: {}\n\
             OS: {} {}\n\
             PID: {}\n\
             Version: 0.12.0\n\n\
             Panic:\n\
             {}\n\n\
             Backtrace:\n\
             {:?}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::process::id(),
            info,
            std::backtrace::Backtrace::capture(),
        );

        let _ = std::fs::write(&crash_path, report);
        eprintln!(
            "[aileron] CRASH REPORT WRITTEN TO: {}",
            crash_path.display()
        );

        // Also print to stderr
        default_hook(info);
    }));
}

/// Log environment info for debugging.
pub fn log_environment() {
    #[cfg(target_os = "linux")]
    {
        tracing::info!(
            "WAYLAND_DISPLAY: {:?}",
            std::env::var("WAYLAND_DISPLAY").ok()
        );
        tracing::info!("DISPLAY: {:?}", std::env::var("DISPLAY").ok());
        tracing::info!(
            "XDG_SESSION_TYPE: {:?}",
            std::env::var("XDG_SESSION_TYPE").ok()
        );
        tracing::info!("GDK_BACKEND: {:?}", std::env::var("GDK_BACKEND").ok());
        tracing::info!(
            "LD_LIBRARY_PATH: {:?}",
            std::env::var("LD_LIBRARY_PATH")
                .ok()
                .map(|v| if v.len() > 80 {
                    format!("{}...(truncated)", &v[..80])
                } else {
                    v
                })
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        tracing::info!("OS: {}", std::env::consts::OS);
        tracing::info!("ARCH: {}", std::env::consts::ARCH);
    }

    // Platform-specific GPU diagnostics
    #[cfg(target_os = "linux")]
    {
        // Check for Vulkan
        if let Ok(output) = std::process::Command::new("vulkaninfo")
            .arg("--summary")
            .output()
        {
            if output.status.success() {
                let summary = String::from_utf8_lossy(&output.stdout);
                for line in summary.lines().take(10) {
                    tracing::info!("vulkaninfo: {}", line.trim());
                }
            } else {
                tracing::warn!(
                    "vulkaninfo failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            tracing::warn!("vulkaninfo not found — Vulkan may not be available");
        }

        // Check for GPU via glxinfo
        if let Ok(output) = std::process::Command::new("glxinfo").arg("-B").output()
            && output.status.success()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines().take(5) {
                tracing::info!("glxinfo: {}", line.trim());
            }
        }

        // Check Vulkan ICDs
        if let Ok(output) = std::process::Command::new("ls")
            .arg("/usr/share/vulkan/icd.d/")
            .output()
            && output.status.success()
        {
            let icds = String::from_utf8_lossy(&output.stdout);
            tracing::info!("Vulkan ICDs: {}", icds.trim());
        }
        if let Ok(output) = std::process::Command::new("ls")
            .arg("/etc/vulkan/icd.d/")
            .output()
            && output.status.success()
        {
            let icds = String::from_utf8_lossy(&output.stdout);
            tracing::info!("Vulkan ICDs (etc): {}", icds.trim());
        }
    }

    #[cfg(target_os = "windows")]
    {
        tracing::info!("Platform: Windows (GPU diagnostics via DirectX)");
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType"])
            .output()
        {
            let info = String::from_utf8_lossy(&output.stdout);
            for line in info.lines().take(10) {
                tracing::info!("GPU: {}", line.trim());
            }
        }
    }
}

/// Main entry point. Extracted from main() to reduce main.rs line count.
/// References `AileronApp` which is defined in the parent module (main.rs).
pub fn run() -> anyhow::Result<()> {
    use tracing::info;
    use winit::event_loop::EventLoop;

    use aileron::config::Config;
    #[cfg(target_os = "linux")]
    use aileron::platform::x11::x11_error_handler;
    use aileron::servo::init_gtk;

    use super::event_handlers::is_nvidia_gpu;

    // Install panic hook BEFORE anything else — writes crash report to file
    install_panic_hook();

    // Initialize debug capturer (no-op unless AILERON_DEBUG=1)
    aileron::debug_capturer::init();

    // Initialize tracing to both stderr AND a log file
    let log_dir = directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("./logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file_path = log_dir.join(format!(
        "aileron_{}.log",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));
    let log_file = std::fs::File::create(&log_file_path).ok();
    if log_file.is_some() {
        eprintln!("[aileron] Logging to: {}", log_file_path.display());
    }

    // Build subscriber writing to both stderr AND a log file
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "aileron=debug,wgpu=warn,wry=debug,webkit2gtk=debug,gdk=debug,gtk=debug,egui=info"
            .parse()
            .expect("hardcoded fallback env filter is valid")
    });

    if let Some(file) = log_file {
        use std::sync::Arc;
        use tracing_subscriber::Layer as _;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(env_filter.clone());
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(Arc::new(file))
            .with_ansi(false)
            .with_filter(env_filter);

        tracing_subscriber::registry()
            .with(stderr_layer)
            .with(file_layer)
            .init();
    } else {
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::DEBUG)
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    }

    info!("Aileron v{}", env!("CARGO_PKG_VERSION"));
    info!("Keyboard-Driven Web Environment");
    info!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    info!("PID: {}", std::process::id());

    // Log environment info
    log_environment();

    // Phase 1: Load config
    info!("-- Phase 1: Loading config --");
    let phase_0 = std::time::Instant::now();
    let config = Config::load();
    info!("Config loaded in {:?}", phase_0.elapsed());
    info!(
        "Config loaded: render_mode={}, tab_layout={}, theme={}",
        config.render_mode, config.tab_layout, config.theme
    );

    // Disable WebKitGTK's DMA-BUF renderer on NVIDIA to prevent
    // "GDK is not able to create a GL context" fatal error on Wayland.
    // Falls back to the shared GL texture path which works on all GPUs.
    // Only set this on NVIDIA GPUs — AMD/Intel benefit from DMA-BUF.
    #[cfg(target_os = "linux")]
    // SAFETY: This runs before any threads are spawned (before event loop creation).
    // WebKitGTK reads these env vars during init, which happens later on the main thread.
    unsafe {
        if is_nvidia_gpu() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            info!("NVIDIA GPU detected -- disabled DMA-BUF renderer (shared GL fallback)");
        }
        // In offscreen mode, WebKitGTK's GL compositor renders to textures
        // that can't be captured via snapshot() or pixbuf() in an
        // OffscreenWindow — both return blank surfaces. Disable compositing
        // to force software rendering, which is capture-compatible.
        if config.render_mode == "offscreen" {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            info!("Offscreen mode -- disabled WebKitGTK compositing for capture compatibility");
        }
    }

    // On NVIDIA + Wayland, winit's Wayland backend (sctk) fails to
    // dispatch keyboard/mouse events despite the compositor sending
    // wl_keyboard.enter. WINIT_UNIX_BACKEND was removed in winit 0.30;
    // backend selection is now based on WAYLAND_DISPLAY presence.
    // Temporarily hide WAYLAND_DISPLAY to force winit onto X11/XWayland.
    #[cfg(target_os = "linux")]
    let wayland_display_backup = {
        if is_nvidia_gpu() && std::env::var("WAYLAND_DISPLAY").is_ok() {
            let backup = std::env::var("WAYLAND_DISPLAY").ok();
            // SAFETY: This runs before any threads are spawned (before event loop creation).
            unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
            info!(
                "NVIDIA + Wayland: temporarily hiding WAYLAND_DISPLAY to force winit X11 backend"
            );
            backup
        } else {
            None
        }
    };
    #[cfg(not(target_os = "linux"))]
    let wayland_display_backup: Option<String> = None;

    // Defer GTK init to AFTER winit event loop creation.
    // GTK's X11 event filter intercepts keyboard/mouse events from the
    // shared X11 event queue, preventing winit from receiving them.
    // By initializing GTK after the event loop, winit's X11 connection
    // is established first and gets its own event filter priority.
    info!("-- Phase 3: Creating event loop --");

    let event_loop = EventLoop::builder().build()?;
    info!("Event loop created successfully");

    // NOTE: On NVIDIA + Wayland, keep WAYLAND_DISPLAY hidden so winit
    // stays on the X11 backend. WebKitGTK will use its own Wayland
    // connection (opened during gtk::init below) independently.
    if wayland_display_backup.is_some() {
        info!("WAYLAND_DISPLAY kept hidden (winit locked to X11 backend)");
    }

    // Workaround: X11 error handler (GTK uses XWayland on Wayland systems)
    #[cfg(target_os = "linux")]
    {
        // SAFETY: FFI call to XSetErrorHandler. xlib handle is checked via `if let Ok()`.
        unsafe {
            if let Ok(xlib) = x11_dl::xlib::Xlib::open() {
                (xlib.XSetErrorHandler)(Some(x11_error_handler));
                info!("X11 error handler installed");
            }
        }
    }

    // Phase 2: Initialize GTK (DEFERRED -- after winit event loop).
    // GTK's X11 event filter must be installed AFTER winit's X11
    // connection, otherwise GTK intercepts all keyboard/mouse events.
    info!("-- Phase 2: Initializing GTK (deferred) --");
    let phase_gtk = std::time::Instant::now();
    init_gtk();
    info!("GTK initialized in {:?}", phase_gtk.elapsed());

    // Phase 4: Create app and run
    info!("-- Phase 4: Creating application --");
    Config::set_session_active();
    let mut app = super::AileronApp::new();
    info!("Application created successfully");

    info!("-- Phase 5: Entering event loop --");
    let event_loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        event_loop.run_app(&mut app)
    }));

    match event_loop_result {
        Ok(Ok(())) => {
            info!("Aileron shutting down.");
        }
        Ok(Err(e)) => {
            tracing::error!("Event loop error: {}", e);
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!("Event loop panicked: {}", msg);
            aileron::debug_capturer::capture_info(&format!("Event loop panic caught: {msg}"));
        }
    }

    aileron::debug_capturer::shutdown();
    Ok(())
}
