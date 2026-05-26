use super::*;
use egui::WidgetType;

#[test]
fn truncate_short_string_unchanged() {
    assert_eq!(truncate_str("hi", 10), "hi");
    assert!(matches!(
        truncate_str("hi", 10),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn truncate_exact_length_unchanged() {
    assert_eq!(truncate_str("hello", 5), "hello");
}

#[test]
fn truncate_over_length_appends_ellipsis() {
    assert_eq!(truncate_str("hello world", 5), "hello...");
    assert!(matches!(
        truncate_str("hello world", 5),
        std::borrow::Cow::Owned(_)
    ));
}

#[test]
fn truncate_multibyte_utf8_preserved() {
    let s = "こんにちは世界";
    assert_eq!(truncate_str(s, 5), "こんにちは...");
}

#[test]
fn truncate_empty_string() {
    assert_eq!(truncate_str("", 10), "");
}

#[test]
fn truncate_max_chars_zero() {
    assert_eq!(truncate_str("hello", 0), "...");
}

#[test]
fn truncate_single_char() {
    assert_eq!(truncate_str("a", 1), "a");
    assert_eq!(truncate_str("a", 0), "...");
}

#[test]
fn a11y_info_correct_type_and_label() {
    let info = a11y_info(WidgetType::Button, "Click me");
    assert!(matches!(info.typ, WidgetType::Button));
    assert_eq!(info.label.as_deref(), Some("Click me"));
}
