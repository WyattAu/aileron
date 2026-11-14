#[cfg(target_os = "linux")]
pub fn is_nvidia_gpu() -> bool {
    (0..=9).any(|i| {
        let path = format!("/sys/class/drm/card{i}/device/vendor");
        std::fs::read_to_string(&path)
            .map(|v| v.trim() == "0x10de")
            .unwrap_or(false)
    })
}

#[cfg(not(target_os = "linux"))]
pub fn is_nvidia_gpu() -> bool {
    false
}
