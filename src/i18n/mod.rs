use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::RwLock;

pub mod loader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    English,
    Chinese,
    Japanese,
    Korean,
    German,
    French,
    Spanish,
    Portuguese,
    Russian,
}

impl Locale {
    pub fn code(&self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::Chinese => "zh",
            Locale::Japanese => "ja",
            Locale::Korean => "ko",
            Locale::German => "de",
            Locale::French => "fr",
            Locale::Spanish => "es",
            Locale::Portuguese => "pt",
            Locale::Russian => "ru",
        }
    }

    pub fn from_code(code: &str) -> Option<Locale> {
        match code {
            "en" => Some(Locale::English),
            "zh" | "zh-CN" | "zh-cn" | "zh_Hans" | "zh-Hans" => Some(Locale::Chinese),
            "ja" => Some(Locale::Japanese),
            "ko" => Some(Locale::Korean),
            "de" => Some(Locale::German),
            "fr" => Some(Locale::French),
            "es" => Some(Locale::Spanish),
            "pt" => Some(Locale::Portuguese),
            "ru" => Some(Locale::Russian),
            _ => None,
        }
    }
}

static LOCALE_OVERRIDE: RwLock<Option<Locale>> = RwLock::new(None);

pub fn set_locale(locale: Locale) {
    if let Ok(mut guard) = LOCALE_OVERRIDE.write() {
        *guard = Some(locale);
    }
}

fn get_locale_override() -> Option<Locale> {
    LOCALE_OVERRIDE.read().ok().and_then(|guard| *guard)
}

pub fn clear_locale_override() {
    if let Ok(mut guard) = LOCALE_OVERRIDE.write() {
        *guard = None;
    }
}

pub fn detect_locale() -> Locale {
    if let Some(locale) = get_locale_override() {
        return locale;
    }
    match std::env::var("LANG").unwrap_or_default().to_lowercase() {
        lang if lang.starts_with("zh") => Locale::Chinese,
        lang if lang.starts_with("ja") => Locale::Japanese,
        lang if lang.starts_with("ko") => Locale::Korean,
        lang if lang.starts_with("de") => Locale::German,
        lang if lang.starts_with("fr") => Locale::French,
        lang if lang.starts_with("es") => Locale::Spanish,
        lang if lang.starts_with("pt") => Locale::Portuguese,
        lang if lang.starts_with("ru") => Locale::Russian,
        lang if lang.starts_with("en") => Locale::English,
        _ => Locale::English,
    }
}

pub fn available_locales() -> Vec<(Locale, &'static str)> {
    vec![
        (Locale::English, "English"),
        (Locale::Chinese, "简体中文"),
        (Locale::Japanese, "日本語"),
        (Locale::Korean, "한국어"),
        (Locale::German, "Deutsch"),
        (Locale::French, "Français"),
        (Locale::Spanish, "Español"),
        (Locale::Portuguese, "Português"),
        (Locale::Russian, "Русский"),
    ]
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

    for (locale_code, strings) in loader::load_all() {
        let locale = match Locale::from_code(locale_code) {
            Some(l) => l,
            None => continue,
        };
        for (key, value) in strings {
            let tr_key = TrKey(Box::leak(key.into_boxed_str()));
            let tr_val: &'static str = Box::leak(value.into_boxed_str());
            map.entry(tr_key)
                .or_insert_with(HashMap::new)
                .insert(locale, tr_val);
        }
    }

    let _ = I18N.set(map);
}

fn register(map: &mut HashMap<TrKey, HashMap<Locale, &'static str>>, key: TrKey, en: &'static str) {
    let mut locales = HashMap::new();
    locales.insert(Locale::English, en);
    map.insert(key, locales);
}

pub fn tr(key: TrKey) -> &'static str {
    let locale = detect_locale();
    I18N.get()
        .and_then(|m| m.get(&key))
        .and_then(|locales| {
            locales
                .get(&locale)
                .or_else(|| locales.get(&Locale::English))
        })
        .copied()
        .unwrap_or(key.0)
}

pub fn tr_locale(key: TrKey, locale: Locale) -> &'static str {
    I18N.get()
        .and_then(|m| m.get(&key))
        .and_then(|locales| {
            locales
                .get(&locale)
                .or_else(|| locales.get(&Locale::English))
        })
        .copied()
        .unwrap_or(key.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_locale_english() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::English);
    }

    #[test]
    fn test_detect_locale_fallback() {
        clear_locale_override();
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(detect_locale(), Locale::English);
    }

    #[test]
    fn test_detect_locale_chinese() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "zh_CN.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Chinese);
    }

    #[test]
    fn test_detect_locale_japanese() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "ja_JP.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Japanese);
    }

    #[test]
    fn test_detect_locale_korean() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "ko_KR.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Korean);
    }

    #[test]
    fn test_detect_locale_german() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "de_DE.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::German);
    }

    #[test]
    fn test_detect_locale_french() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "fr_FR.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::French);
    }

    #[test]
    fn test_detect_locale_spanish() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "es_ES.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Spanish);
    }

    #[test]
    fn test_detect_locale_portuguese() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "pt_BR.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Portuguese);
    }

    #[test]
    fn test_detect_locale_russian() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "ru_RU.UTF-8");
        }
        assert_eq!(detect_locale(), Locale::Russian);
    }

    #[test]
    fn test_set_locale_override() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        set_locale(Locale::Chinese);
        assert_eq!(detect_locale(), Locale::Chinese);
        clear_locale_override();
    }

    #[test]
    fn test_set_locale_override_takes_precedence() {
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "zh_CN.UTF-8");
        }
        set_locale(Locale::German);
        assert_eq!(detect_locale(), Locale::German);
        clear_locale_override();
    }

    #[test]
    fn test_locale_code_roundtrip() {
        for (locale, _name) in available_locales() {
            let code = locale.code();
            assert_eq!(Locale::from_code(code), Some(locale));
        }
    }

    #[test]
    fn test_locale_from_code_unknown() {
        assert_eq!(Locale::from_code("xx"), None);
        assert_eq!(Locale::from_code(""), None);
    }

    #[test]
    fn test_available_locales() {
        let locales = available_locales();
        assert_eq!(locales.len(), 9);
        assert_eq!(locales[0], (Locale::English, "English"));
    }

    #[test]
    fn test_toml_loading() {
        let translations = loader::load_all();
        assert!(!translations.is_empty());
        let en = translations.iter().find(|(code, _)| *code == "en").unwrap();
        assert_eq!(en.1.get("mode_normal").unwrap(), "NORMAL");
        assert_eq!(en.1.get("cmd_quit").unwrap(), "Quit Aileron");
    }

    #[test]
    fn test_toml_all_locales_present() {
        let translations = loader::load_all();
        let codes: Vec<&str> = translations.iter().map(|(c, _)| *c).collect();
        for expected in &["en", "zh", "ja", "ko", "de", "fr", "es", "pt", "ru"] {
            assert!(codes.contains(expected), "Missing locale: {}", expected);
        }
    }

    #[test]
    fn test_toml_chinese_translations() {
        let translations = loader::load_all();
        let zh = translations.iter().find(|(code, _)| *code == "zh").unwrap();
        assert_eq!(zh.1.get("mode_normal").unwrap(), "普通");
        assert_eq!(zh.1.get("cmd_quit").unwrap(), "退出 Aileron");
    }

    #[test]
    fn test_toml_japanese_translations() {
        let translations = loader::load_all();
        let ja = translations.iter().find(|(code, _)| *code == "ja").unwrap();
        assert_eq!(ja.1.get("mode_normal").unwrap(), "ノーマル");
    }

    #[test]
    fn test_toml_german_translations() {
        let translations = loader::load_all();
        let de = translations.iter().find(|(code, _)| *code == "de").unwrap();
        assert_eq!(de.1.get("mode_normal").unwrap(), "NORMAL");
        assert_eq!(de.1.get("cmd_quit").unwrap(), "Aileron beenden");
    }

    #[test]
    fn test_toml_spanish_translations() {
        let translations = loader::load_all();
        let es = translations.iter().find(|(code, _)| *code == "es").unwrap();
        assert_eq!(es.1.get("cmd_quit").unwrap(), "Salir de Aileron");
    }

    #[test]
    fn test_tr_fallback() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("unknown_key")), "unknown_key");
    }

    #[test]
    fn test_tr_known_key() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
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
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("status_pinned")), "Pane pinned");
    }

    #[test]
    fn test_tr_status_unpinned() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("status_unpinned")), "Pane unpinned");
    }

    #[test]
    fn test_tr_cmd_quit() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("cmd_quit")), "Quit Aileron");
    }

    #[test]
    fn test_tr_cmd_new_tab() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("cmd_new_tab")), "New tab");
    }

    #[test]
    fn test_tr_err_vault_locked() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(
            tr(TrKey("err_vault_locked")),
            "Vault locked. Use :bw-unlock"
        );
    }

    #[test]
    fn test_tr_status_blocked() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("status_blocked")), "Blocked by ad blocker");
    }

    #[test]
    fn test_tr_status_credential_saved() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("status_credential_saved")), "Credential saved");
    }

    #[test]
    fn test_tr_status_vault_locked() {
        init();
        clear_locale_override();
        unsafe {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
        assert_eq!(tr(TrKey("status_vault_locked")), "Vault locked");
    }

    #[test]
    fn test_tr_fallback_to_english() {
        init();
        set_locale(Locale::Chinese);
        let val = tr(TrKey("mode_normal"));
        assert_eq!(val, "普通");
        clear_locale_override();
    }

    #[test]
    fn test_tr_locale_fallback_to_english() {
        init();
        let val = tr_locale(TrKey("mode_normal"), Locale::Chinese);
        assert_eq!(val, "普通");
    }

    #[test]
    fn test_tr_missing_locale_falls_back_to_english() {
        init();
        let val = tr_locale(TrKey("err_unknown_command"), Locale::Chinese);
        assert!(val.contains("未知命令") || val.contains("{}"));
    }
}
