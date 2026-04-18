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
pub struct TrKey(&'static str);

static I18N: OnceLock<HashMap<TrKey, HashMap<Locale, &'static str>>> = OnceLock::new();

pub fn init() {
    let mut map = HashMap::new();

    let mut status_normal = HashMap::new();
    status_normal.insert(Locale::English, "NORMAL");
    map.insert(TrKey("status_normal"), status_normal);

    let mut status_insert = HashMap::new();
    status_insert.insert(Locale::English, "INSERT");
    map.insert(TrKey("status_insert"), status_insert);

    let mut status_command = HashMap::new();
    status_command.insert(Locale::English, "COMMAND");
    map.insert(TrKey("status_command"), status_command);

    let mut panes = HashMap::new();
    panes.insert(Locale::English, "panes");
    map.insert(TrKey("panes"), panes);

    let mut hint_mode = HashMap::new();
    hint_mode.insert(Locale::English, "hint");
    map.insert(TrKey("hint_mode"), hint_mode);

    let mut find = HashMap::new();
    find.insert(Locale::English, "Find:");
    map.insert(TrKey("find"), find);

    let mut search_or_enter_url = HashMap::new();
    search_or_enter_url.insert(Locale::English, "Search or enter URL...");
    map.insert(TrKey("search_or_enter_url"), search_or_enter_url);

    let _ = I18N.set(map);
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
        assert_eq!(tr(TrKey("status_normal")), "NORMAL");
    }

    #[test]
    fn test_tr_locale_specific() {
        init();
        assert_eq!(tr_locale(TrKey("status_insert"), Locale::English), "INSERT");
    }
}
