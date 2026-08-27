use super::*;

#[test]
fn parses_supported_locale_aliases() {
    assert_eq!(Locale::parse("zh-Hans-CN"), Some(Locale::ZhCn));
    assert_eq!(Locale::parse("zh_CN"), Some(Locale::ZhCn));
    assert_eq!(Locale::parse("en-US"), Some(Locale::EnUs));
    assert_eq!(Locale::parse("fr-FR"), None);
}

#[test]
fn chinese_is_the_fork_default() {
    assert_eq!(Locale::default(), Locale::ZhCn);
    assert_eq!(Locale::ZhCn.language_tag(), "zh-CN");
}

#[test]
fn static_compatibility_path_translates_only_borrowed_copy() {
    let borrowed = localize_static_for_locale(Locale::ZhCn, Cow::Borrowed("Cancel"));
    let owned = localize_static_for_locale(Locale::ZhCn, Cow::Owned("Cancel".to_owned()));

    assert_eq!(borrowed, "取消");
    assert_eq!(owned, "Cancel");
}

#[test]
fn explicit_ui_path_can_translate_owned_copy() {
    let localized = localize_ui_for_locale(Locale::ZhCn, Cow::Owned("Cancel".to_owned()));
    assert_eq!(localized, "取消");
}

#[test]
fn static_reference_path_supports_borrowing_apis() {
    assert_eq!(localize_ref_for_locale(Locale::ZhCn, "Cancel"), "取消");
    assert_eq!(localize_ref_for_locale(Locale::EnUs, "Cancel"), "Cancel");
}

#[test]
fn localized_templates_preserve_dynamic_values() {
    let formatted = format_ui_for_locale(
        Locale::ZhCn,
        "Could not restore conversation: {message}",
        &[("message", "Cancel".to_owned())],
    );
    assert_eq!(formatted, "无法恢复对话：Cancel");
}

#[test]
fn english_and_unknown_copy_fall_back_without_allocation() {
    let english = localize_ui_for_locale(Locale::EnUs, Cow::Borrowed("Cancel"));
    let unknown = localize_ui_for_locale(Locale::ZhCn, Cow::Borrowed("A user file name"));

    assert_eq!(english, "Cancel");
    assert_eq!(unknown, "A user file name");
}

#[test]
fn embedded_catalog_is_valid_and_substantial() {
    let catalog = parse_catalog(ZH_CN_SOURCE).expect("embedded catalog must be valid");
    assert!(catalog.len() >= 1_200, "catalog unexpectedly shrank");
    assert_eq!(catalog.get("Settings").map(String::as_str), Some("设置"));
}

#[test]
fn duplicate_source_is_a_hard_failure() {
    let error = parse_catalog("Cancel\t取消\nCancel\t取消操作\n")
        .expect_err("duplicate source must be rejected");
    assert_eq!(
        error,
        CatalogError::DuplicateSource {
            line: 2,
            source: "Cancel".to_owned()
        }
    );
}

#[test]
fn catalog_fields_support_newlines_and_literal_backslashes() {
    let catalog = parse_catalog("Line\\nTwo\t行一\\n行二\nPath \\\\ value\t路径 \\\\ 值\n")
        .expect("escaped catalog must parse");
    assert_eq!(
        catalog.get("Line\nTwo").map(String::as_str),
        Some("行一\n行二")
    );
    assert_eq!(
        catalog.get(r"Path \ value").map(String::as_str),
        Some(r"路径 \ 值")
    );
}
