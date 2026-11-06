//! Platform-specific file paths.
//!
//! The directory resolution functions (config_dir, data_dir, cache_dir) use the
//! `directories` crate and work cross-platform without conditional compilation.
//!
//! The per-OS functions (downloads_dir, os_name, etc.) delegate to the
//! [`PlatformOps`](super::PlatformOps) trait via [`super::platform()`].
//! Prefer calling `crate::platform::platform()` directly for new code.

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\Users\Default"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn fallback_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir().join("AppData").join("Roaming").join("Aileron")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".config").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".config").join("aileron")
    }
}

fn fallback_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir().join("AppData").join("Local").join("Aileron")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".local").join("share").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".local").join("share").join("aileron")
    }
}

fn fallback_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir()
            .join("AppData")
            .join("Local")
            .join("Aileron")
            .join("Cache")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library").join("Caches").join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".cache").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".cache").join("aileron")
    }
}

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(fallback_config_dir)
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(fallback_data_dir)
}

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(fallback_cache_dir)
}
