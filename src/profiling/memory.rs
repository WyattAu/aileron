//! Memory usage monitoring.

/// Get current process RSS (Resident Set Size) in bytes.
#[cfg(target_os = "linux")]
pub fn process_rss_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    let trimmed = rest.trim();
                    let num_str: String =
                        trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(kb) = num_str.parse::<u64>() {
                        return Some(kb * 1024);
                    }
                }
            }
            None
        })
}

#[cfg(not(target_os = "linux"))]
pub fn process_rss_bytes() -> Option<u64> {
    None
}

/// Get current process RSS in human-readable format.
pub fn process_rss_human() -> String {
    match process_rss_bytes() {
        Some(bytes) => format_human_bytes(bytes),
        None => "N/A".into(),
    }
}

/// Format bytes as human-readable string.
pub fn format_human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Estimate per-pane memory usage.
/// WebKitGTK webviews typically use 20-80MB each.
/// Native terminals use 2-5MB each.
pub fn estimate_pane_memory(num_web_panes: usize, num_terminal_panes: usize) -> u64 {
    let web_per_pane: u64 = 50 * 1024 * 1024;
    let term_per_pane: u64 = 3 * 1024 * 1024;
    num_web_panes as u64 * web_per_pane + num_terminal_panes as u64 * term_per_pane
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_human_bytes() {
        assert_eq!(format_human_bytes(500), "500 B");
        assert_eq!(format_human_bytes(1024), "1.0 KB");
        assert_eq!(format_human_bytes(1536), "1.5 KB");
        assert_eq!(format_human_bytes(1048576), "1.0 MB");
        assert_eq!(format_human_bytes(1073741824), "1.0 GB");
        assert_eq!(format_human_bytes(1610612736), "1.5 GB");
    }

    #[test]
    fn test_estimate_pane_memory() {
        assert_eq!(estimate_pane_memory(2, 1), 103 * 1024 * 1024);
    }
}
