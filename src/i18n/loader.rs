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
