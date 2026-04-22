use std::path::PathBuf;

pub trait PlatformOps: Send + Sync {
    fn downloads_dir(&self) -> PathBuf;
    fn os_name(&self) -> &'static str;
    fn desktop_environment(&self) -> Option<String>;
    fn is_wayland(&self) -> bool;
    fn is_x11(&self) -> bool;
    fn default_browser_cmd(&self) -> Vec<String>;
    fn default_terminal_cmd(&self) -> Vec<String>;
    fn wry_backend(&self) -> &'static str;
    fn config_overrides(&self) -> Vec<(&'static str, String)>;
    fn file_open_dialog(&self, title: &str, filters: &[(&str, &str)]) -> Option<PathBuf>;
    fn show_notification(&self, title: &str, body: &str);
    fn super_key_name(&self) -> &'static str;

    /// Return the shell command for executing a user-provided command string.
    /// E.g. on Linux returns `["sh", "-c", cmd]`, on Windows `["cmd", "/c", cmd]`.
    fn shell_command(&self, cmd: &str) -> Vec<String>;

    /// Copy text to the system clipboard. Returns true on success.
    fn clipboard_copy(&self, text: &str) -> bool;
}
