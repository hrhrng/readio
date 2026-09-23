use readio_core::samples::builtin_sample;

#[test]
fn builtin_sample_follows_locale_without_changing_its_reserved_identity() {
    let english = builtin_sample("en-US");
    let chinese = builtin_sample("zh-Hans-CN");

    assert_eq!(english.item_id, "readio:sample:v1");
    assert_eq!(chinese.item_id, "readio:sample:v1");
    assert_eq!(english.legacy_item_ids, ["6b4121313cc4a35d"]);
    assert_eq!(english.locale, "en");
    assert_eq!(english.title, "The Shape of Attention");
    assert_eq!(english.author, "readio sample");
    assert_eq!(chinese.locale, "zh");
    assert_eq!(chinese.title, "注意力的形状");
    assert_eq!(chinese.author, "readio 示例");
    assert!(english.is_builtin_sample);
    assert_eq!(english.source, "readio://sample");
    assert!(
        english
            .content
            .starts_with("One  Attention in eighty columns\n")
    );
    assert!(chinese.content.starts_with("一 终端里的注意力\n"));
    assert!(!english.content.contains("# One"));
    assert!(!chinese.content.contains("> 界面不是中立的"));
    assert_ne!(english.content, chinese.content);
}

#[test]
fn unsupported_or_empty_language_tags_use_the_complete_english_sample() {
    for tag in ["", "fr-FR", "not-a-language"] {
        let sample = builtin_sample(tag);
        assert_eq!(sample.locale, "en");
        assert_eq!(sample.title, "The Shape of Attention");
        assert!(
            sample
                .content
                .contains("Four  One continuous piece of work")
        );
    }
}
