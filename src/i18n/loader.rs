use std::collections::HashMap;

pub fn load_all() -> Vec<(&'static str, HashMap<String, String>)> {
    let mut result = Vec::new();

    let en_toml = include_str!("locales/en.toml");
    if let Ok(map) = parse_toml(en_toml) {
        result.push(("en", map));
    }

    let zh_toml = include_str!("locales/zh.toml");
    if let Ok(map) = parse_toml(zh_toml) {
        result.push(("zh", map));
    }

    let ja_toml = include_str!("locales/ja.toml");
    if let Ok(map) = parse_toml(ja_toml) {
        result.push(("ja", map));
    }

    let ko_toml = include_str!("locales/ko.toml");
    if let Ok(map) = parse_toml(ko_toml) {
        result.push(("ko", map));
    }

    let de_toml = include_str!("locales/de.toml");
    if let Ok(map) = parse_toml(de_toml) {
        result.push(("de", map));
    }

    let fr_toml = include_str!("locales/fr.toml");
    if let Ok(map) = parse_toml(fr_toml) {
        result.push(("fr", map));
    }

    let es_toml = include_str!("locales/es.toml");
    if let Ok(map) = parse_toml(es_toml) {
        result.push(("es", map));
    }

    let pt_toml = include_str!("locales/pt.toml");
    if let Ok(map) = parse_toml(pt_toml) {
        result.push(("pt", map));
    }

    let ru_toml = include_str!("locales/ru.toml");
    if let Ok(map) = parse_toml(ru_toml) {
        result.push(("ru", map));
    }

    result
}

fn parse_toml(input: &str) -> Result<HashMap<String, String>, toml::de::Error> {
    toml::from_str(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_returns_all_locales() {
        let translations = load_all();
        let codes: Vec<&str> = translations.iter().map(|(c, _)| *c).collect();
        assert_eq!(codes.len(), 9);
        for expected in &["en", "zh", "ja", "ko", "de", "fr", "es", "pt", "ru"] {
            assert!(codes.contains(expected), "Missing locale: {}", expected);
        }
    }

    #[test]
    fn test_english_translations_complete() {
        let translations = load_all();
        let en = translations
            .iter()
            .find(|(code, _)| *code == "en")
            .expect("English locale should exist");
        assert_eq!(en.1.len(), 32, "English should have all 32 keys");
    }

    #[test]
    fn test_english_expected_keys_present() {
        let translations = load_all();
        let en = translations.iter().find(|(code, _)| *code == "en").unwrap();
        let expected_keys = [
            "mode_normal",
            "mode_insert",
            "mode_command",
            "panes",
            "hint_mode",
            "find",
            "search_or_enter_url",
            "status_saved",
            "status_restored",
            "status_pinned",
            "status_unpinned",
            "status_blocked",
            "status_credential_saved",
            "status_filter_updated",
            "status_no_credential",
            "status_vault_locked",
            "status_profiling_on",
            "status_profiling_off",
            "cmd_quit",
            "cmd_close",
            "cmd_split_v",
            "cmd_split_h",
            "cmd_new_tab",
            "cmd_settings",
            "cmd_adblock_update",
            "cmd_print",
            "cmd_memory",
            "cmd_perf",
            "cmd_credentials",
            "err_unknown_command",
            "err_vault_locked",
            "err_save_failed",
        ];
        for key in &expected_keys {
            assert!(en.1.contains_key(*key), "Missing English key: {}", key);
        }
    }

    #[test]
    fn test_english_values_non_empty() {
        let translations = load_all();
        let en = translations.iter().find(|(code, _)| *code == "en").unwrap();
        for (key, value) in &en.1 {
            assert!(
                !value.is_empty(),
                "English translation for '{}' should not be empty",
                key
            );
        }
    }

    #[test]
    fn test_parse_toml_valid() {
        let input = r#"
key1 = "value1"
key2 = "value2"
"#;
        let result = parse_toml(input).unwrap();
        assert_eq!(result.get("key1").unwrap(), "value1");
        assert_eq!(result.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_parse_toml_empty() {
        let result = parse_toml("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_toml_invalid() {
        let result = parse_toml("this is not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn test_all_locales_non_empty() {
        let translations = load_all();
        for (code, map) in &translations {
            assert!(!map.is_empty(), "Locale '{}' should not be empty", code);
        }
    }

    #[test]
    fn test_chinese_has_translations() {
        let translations = load_all();
        let zh = translations
            .iter()
            .find(|(code, _)| *code == "zh")
            .expect("Chinese locale should exist");
        assert_eq!(zh.1.get("mode_normal").unwrap(), "普通");
    }

    #[test]
    fn test_japanese_has_translations() {
        let translations = load_all();
        let ja = translations
            .iter()
            .find(|(code, _)| *code == "ja")
            .expect("Japanese locale should exist");
        assert_eq!(ja.1.get("mode_normal").unwrap(), "ノーマル");
    }
}
