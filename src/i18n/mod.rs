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

    #[must_use]
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

/// Parse a LANG-style environment string into a Locale.
/// Pure function -- no env var access, safe for concurrent tests.
fn parse_lang_env(lang: &str) -> Locale {
    let lang = lang.to_lowercase();
    match lang.as_str() {
        l if l.starts_with("zh") => Locale::Chinese,
        l if l.starts_with("ja") => Locale::Japanese,
        l if l.starts_with("ko") => Locale::Korean,
        l if l.starts_with("de") => Locale::German,
        l if l.starts_with("fr") => Locale::French,
        l if l.starts_with("es") => Locale::Spanish,
        l if l.starts_with("pt") => Locale::Portuguese,
        l if l.starts_with("ru") => Locale::Russian,
        l if l.starts_with("en") => Locale::English,
        _ => Locale::English,
    }
}

pub fn detect_locale() -> Locale {
    if let Some(locale) = get_locale_override() {
        return locale;
    }
    parse_lang_env(&std::env::var("LANG").unwrap_or_default())
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

    // --- Pure locale parsing tests (no env var mutation, safe for parallelism) ---

    #[test]
    fn test_parse_lang_env_english() {
        assert_eq!(parse_lang_env("en_US.UTF-8"), Locale::English);
    }

    #[test]
    fn test_parse_lang_env_fallback() {
        assert_eq!(parse_lang_env(""), Locale::English);
        assert_eq!(parse_lang_env("C"), Locale::English);
        assert_eq!(parse_lang_env("POSIX"), Locale::English);
    }

    #[test]
    fn test_parse_lang_env_chinese() {
        assert_eq!(parse_lang_env("zh_CN.UTF-8"), Locale::Chinese);
    }

    #[test]
    fn test_parse_lang_env_japanese() {
        assert_eq!(parse_lang_env("ja_JP.UTF-8"), Locale::Japanese);
    }

    #[test]
    fn test_parse_lang_env_korean() {
        assert_eq!(parse_lang_env("ko_KR.UTF-8"), Locale::Korean);
    }

    #[test]
    fn test_parse_lang_env_german() {
        assert_eq!(parse_lang_env("de_DE.UTF-8"), Locale::German);
    }

    #[test]
    fn test_parse_lang_env_french() {
        assert_eq!(parse_lang_env("fr_FR.UTF-8"), Locale::French);
    }

    #[test]
    fn test_parse_lang_env_spanish() {
        assert_eq!(parse_lang_env("es_ES.UTF-8"), Locale::Spanish);
    }

    #[test]
    fn test_parse_lang_env_portuguese() {
        assert_eq!(parse_lang_env("pt_BR.UTF-8"), Locale::Portuguese);
    }

    #[test]
    fn test_parse_lang_env_russian() {
        assert_eq!(parse_lang_env("ru_RU.UTF-8"), Locale::Russian);
    }

    #[test]
    fn test_parse_lang_env_case_insensitive() {
        assert_eq!(parse_lang_env("JA_JP.UTF-8"), Locale::Japanese);
        assert_eq!(parse_lang_env("Zh_CN.utf-8"), Locale::Chinese);
    }

    // --- Locale override tests (use LOCALE_OVERRIDE, not env vars) ---

    #[test]
    fn test_set_locale_override() {
        set_locale(Locale::Chinese);
        // Override takes precedence regardless of what detect_locale would read from env.
        assert_eq!(detect_locale(), Locale::Chinese);
        clear_locale_override();
    }

    #[test]
    fn test_set_locale_override_takes_precedence() {
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
            assert!(codes.contains(expected), "Missing locale: {expected}");
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
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("unknown_key")), "unknown_key");
        clear_locale_override();
    }

    #[test]
    fn test_tr_known_key() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("mode_normal")), "NORMAL");
        clear_locale_override();
    }

    #[test]
    fn test_tr_locale_specific() {
        init();
        assert_eq!(tr_locale(TrKey("mode_insert"), Locale::English), "INSERT");
    }

    #[test]
    fn test_tr_status_pinned() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("status_pinned")), "Pane pinned");
        clear_locale_override();
    }

    #[test]
    fn test_tr_status_unpinned() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("status_unpinned")), "Pane unpinned");
        clear_locale_override();
    }

    #[test]
    fn test_tr_cmd_quit() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("cmd_quit")), "Quit Aileron");
        clear_locale_override();
    }

    #[test]
    fn test_tr_cmd_new_tab() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("cmd_new_tab")), "New tab");
        clear_locale_override();
    }

    #[test]
    fn test_tr_err_vault_locked() {
        init();
        set_locale(Locale::English);
        assert_eq!(
            tr(TrKey("err_vault_locked")),
            "Vault locked. Use :bw-unlock"
        );
        clear_locale_override();
    }

    #[test]
    fn test_tr_status_blocked() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("status_blocked")), "Blocked by ad blocker");
        clear_locale_override();
    }

    #[test]
    fn test_tr_status_credential_saved() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("status_credential_saved")), "Credential saved");
        clear_locale_override();
    }

    #[test]
    fn test_tr_status_vault_locked() {
        init();
        set_locale(Locale::English);
        assert_eq!(tr(TrKey("status_vault_locked")), "Vault locked");
        clear_locale_override();
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
