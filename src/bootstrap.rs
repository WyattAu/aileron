//! Application bootstrap and diagnostics utilities.

use std::path::PathBuf;

/// Command-line arguments parsed from std::env::args().
pub struct CliArgs {
    /// Enable debug logging (sets RUST_LOG=debug).
    pub debug: bool,
    /// Custom profiling output directory.
    pub profile_dir: Option<PathBuf>,
    /// Dump current config and exit.
    pub dump_config: bool,
    /// Enable test harness mode with optional output directory.
    pub test_harness: Option<PathBuf>,
    /// Print DOM JSON to stdout after each capture (requires --test-harness).
    pub dump_dom: bool,
    /// Use comprehensive test route instead of default route.
    pub comprehensive_test: bool,
}

impl CliArgs {
    /// Parse command-line arguments from std::env::args().
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut debug = false;
        let mut profile_dir = None;
        let mut dump_config = false;
        let mut test_harness = None;
        let mut dump_dom = false;
        let mut comprehensive_test = false;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--debug" | "-d" => {
                    debug = true;
                }
                "--profile" | "-p" => {
                    i += 1;
                    if let Some(dir) = args.get(i) {
                        profile_dir = Some(PathBuf::from(dir));
                    }
                }
                "--dump-config" => {
                    dump_config = true;
                }
                "--test-harness" => {
                    i += 1;
                    let dir = args
                        .get(i)
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("/tmp/aileron_test_output"));
                    test_harness = Some(dir);
                }
                "--comprehensive" => {
                    comprehensive_test = true;
                }
                "--dump-dom" => {
                    dump_dom = true;
                }
                "--help" | "-h" => {
                    eprintln!("Usage: aileron [OPTIONS]");
                    eprintln!();
                    eprintln!("Options:");
                    eprintln!("  --debug, -d              Enable debug logging");
                    eprintln!("  --profile <dir>          Set profiling output directory");
                    eprintln!("  --dump-config            Print current config and exit");
                    eprintln!("  --test-harness [dir]     Run internal test harness");
                    eprintln!(
                        "  --dump-dom               Print DOM JSON to stdout (with --test-harness)"
                    );
                    eprintln!("  --help, -h               Show this help message");
                    std::process::exit(0);
                }
                "--version" | "-V" => {
                    eprintln!("aileron {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                _ => {
                    // Ignore unknown arguments
                }
            }
            i += 1;
        }

        Self {
            debug,
            profile_dir,
            dump_config,
            test_harness,
            dump_dom,
            comprehensive_test,
        }
    }
}

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

    // D1-05: Parse CLI arguments
    let cli_args = CliArgs::parse();

    // Install panic hook BEFORE anything else — writes crash report to file
    install_panic_hook();

    // Initialize debug capturer (no-op unless AILERON_DEBUG=1)
    aileron::debug_capturer::init();

    // D1-05: If --dump-config, print config and exit
    if cli_args.dump_config {
        let config = Config::load();
        match serde_json::to_string_pretty(&config) {
            Ok(json) => {
                println!("{json}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Failed to serialize config: {e}");
                std::process::exit(1);
            }
        }
    }

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
    // D1-05: Use debug level if --debug flag is set
    let env_filter = if cli_args.debug {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "aileron=debug,wgpu=debug,wry=debug,webkit2gtk=debug,gdk=debug,gtk=debug,egui=debug"
                .parse()
                .expect("hardcoded fallback env filter is valid")
        })
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "aileron=debug,wgpu=warn,wry=debug,webkit2gtk=debug,gdk=debug,gtk=debug,egui=info"
                .parse()
                .expect("hardcoded fallback env filter is valid")
        })
    };

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

    // D1-05: Log profiling directory if set
    if let Some(ref profile_dir) = cli_args.profile_dir {
        info!("Profiling output directory: {}", profile_dir.display());
        let _ = std::fs::create_dir_all(profile_dir);
    }

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
        // WEBKIT_DISABLE_COMPOSITING_MODE was previously set here to force
        // software rendering for pixbuf capture. This prevented HTTPS pages
        // from rendering (composited content never reached the OffscreenWindow
        // surface). The snapshot() API (WebKitGTK 2.38+) correctly captures
        // GL-composited content, so compositing must remain enabled.
        if config.render_mode == "offscreen" {
            info!("Offscreen mode -- WebKitGTK compositing enabled (snapshot capture)");
        }
    }

    // On NVIDIA + Wayland, winit's Wayland backend (sctk) previously failed to
    // dispatch keyboard/mouse events. We used to hide WAYLAND_DISPLAY to force
    // winit onto X11/XWayland, but this caused X11 child window compositing
    // issues. Instead, we now use native Wayland and let winit handle the
    // event loop. GTK/Wayland handles its own rendering via WebKitGTK.
    #[cfg(target_os = "linux")]
    let wayland_display_backup: Option<String> = None;
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

    // Initialize test harness if requested
    if let Some(ref test_dir) = cli_args.test_harness {
        app.init_test_harness(test_dir, cli_args.dump_dom, cli_args.comprehensive_test);
    }

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
