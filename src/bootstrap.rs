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
        let crash_path = crash_dir.join(format!("crash_{}.txt", timestamp));

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
