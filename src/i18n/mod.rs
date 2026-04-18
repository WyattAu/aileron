use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    English,
}

pub fn detect_locale() -> Locale {
    match std::env::var("LANG").unwrap_or_default().to_lowercase() {
        lang if lang.starts_with("en") => Locale::English,
        _ => Locale::English,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrKey(pub &'static str);

static I18N: OnceLock<HashMap<TrKey, HashMap<Locale, &'static str>>> = OnceLock::new();

pub fn init() {
    let mut map = HashMap::new();

    register(&mut map, TrKey("mode_normal"), "NORMAL");
    register(&mut map, TrKey("mode_insert"), "INSERT");
    register(&mut map, TrKey("mode_command"), "COMMAND");

    register(&mut map, TrKey("panes"), "panes");
    register(&mut map, TrKey("hint_mode"), "hint");
    register(&mut map, TrKey("find"), "Find:");
    register(
        &mut map,
        TrKey("search_or_enter_url"),
        "Search or enter URL...",
    );

    register(&mut map, TrKey("status_saved"), "Workspace saved");
    register(&mut map, TrKey("status_restored"), "Workspace restored");
    register(&mut map, TrKey("status_pinned"), "Pane pinned");
    register(&mut map, TrKey("status_unpinned"), "Pane unpinned");
    register(&mut map, TrKey("status_blocked"), "Blocked by ad blocker");
    register(
        &mut map,
        TrKey("status_credential_saved"),
        "Credential saved",
    );
    register(
        &mut map,
        TrKey("status_filter_updated"),
        "Filter lists updated",
    );
    register(
        &mut map,
        TrKey("status_no_credential"),
        "No credentials for this site",
    );
    register(&mut map, TrKey("status_vault_locked"), "Vault locked");
    register(&mut map, TrKey("status_profiling_on"), "Profiling enabled");
    register(
        &mut map,
        TrKey("status_profiling_off"),
        "Profiling disabled",
    );

    register(&mut map, TrKey("cmd_quit"), "Quit Aileron");
    register(&mut map, TrKey("cmd_close"), "Close pane");
    register(&mut map, TrKey("cmd_split_v"), "Split vertical");
    register(&mut map, TrKey("cmd_split_h"), "Split horizontal");
    register(&mut map, TrKey("cmd_new_tab"), "New tab");
    register(&mut map, TrKey("cmd_settings"), "Open settings");
    register(&mut map, TrKey("cmd_adblock_update"), "Update filter lists");
    register(&mut map, TrKey("cmd_print"), "Print page");
    register(&mut map, TrKey("cmd_memory"), "Show memory usage");
    register(&mut map, TrKey("cmd_perf"), "Show performance stats");
    register(&mut map, TrKey("cmd_credentials"), "Search credentials");

    register(
        &mut map,
        TrKey("err_unknown_command"),
        "Unknown command: {}",
    );
    register(
        &mut map,
        TrKey("err_vault_locked"),
        "Vault locked. Use :bw-unlock",
    );
    register(&mut map, TrKey("err_save_failed"), "Failed to save: {}");

    let _ = I18N.set(map);
}

fn register(map: &mut HashMap<TrKey, HashMap<Locale, &'static str>>, key: TrKey, en: &'static str) {
    let mut locales = HashMap::new();
    locales.insert(Locale::English, en);
    map.insert(key, locales);
}

pub fn tr(key: TrKey) -> &'static str {
    I18N.get()
        .and_then(|m| m.get(&key))
        .and_then(|locales| locales.get(&detect_locale()))
        .copied()
        .unwrap_or(key.0)
}

pub fn tr_locale(key: TrKey, locale: Locale) -> &'static str {
    I18N.get()
        .and_then(|m| m.get(&key))
        .and_then(|locales| locales.get(&locale))
        .copied()
        .unwrap_or(key.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_locale_english() {
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::English);
    }

    #[test]
    fn test_detect_locale_fallback() {
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(detect_locale(), Locale::English);
    }

    #[test]
    fn test_tr_fallback() {
        init();
        assert_eq!(tr(TrKey("unknown_key")), "unknown_key");
    }

    #[test]
    fn test_tr_known_key() {
        init();
        assert_eq!(tr(TrKey("mode_normal")), "NORMAL");
    }

    #[test]
    fn test_tr_locale_specific() {
        init();
        assert_eq!(tr_locale(TrKey("mode_insert"), Locale::English), "INSERT");
    }

    #[test]
    fn test_tr_status_pinned() {
        init();
        assert_eq!(tr(TrKey("status_pinned")), "Pane pinned");
    }

    #[test]
    fn test_tr_status_unpinned() {
        init();
        assert_eq!(tr(TrKey("status_unpinned")), "Pane unpinned");
    }

    #[test]
    fn test_tr_cmd_quit() {
        init();
        assert_eq!(tr(TrKey("cmd_quit")), "Quit Aileron");
    }

    #[test]
    fn test_tr_cmd_new_tab() {
        init();
        assert_eq!(tr(TrKey("cmd_new_tab")), "New tab");
    }

    #[test]
    fn test_tr_err_vault_locked() {
        init();
        assert_eq!(
            tr(TrKey("err_vault_locked")),
            "Vault locked. Use :bw-unlock"
        );
    }

    #[test]
    fn test_tr_status_blocked() {
        init();
        assert_eq!(tr(TrKey("status_blocked")), "Blocked by ad blocker");
    }

    #[test]
    fn test_tr_status_credential_saved() {
        init();
        assert_eq!(tr(TrKey("status_credential_saved")), "Credential saved");
    }

    #[test]
    fn test_tr_status_vault_locked() {
        init();
        assert_eq!(tr(TrKey("status_vault_locked")), "Vault locked");
    }
}
