use readio::{
    book::Book,
    i18n,
    store::{Progress, Store},
};
use readio_sdk::ReadioSdk;

#[test]
fn tui_sample_is_the_shared_virtual_document() {
    let language_tag = match i18n::current() {
        i18n::Lang::Zh => "zh",
        i18n::Lang::En => "en",
    };
    let shared = ReadioSdk::new().samples.resolve(language_tag);
    let book = Book::load(None).expect("built-in sample");

    assert_eq!(book.id, "readio:sample:v1");
    assert_eq!(book.title, shared.title);
    assert_eq!(book.author.as_deref(), Some(shared.author.as_str()));
    assert_eq!(book.chapters.len(), 4);
    assert!(shared.markdown_source.starts_with("# "));
    assert_eq!(
        book.chapters[0].paras[0].text(),
        if language_tag == "zh" {
            "一 终端里的注意力"
        } else {
            "One Attention in eighty columns"
        }
    );
}

#[test]
fn tui_migrates_legacy_sample_progress_to_the_reserved_identity() {
    let mut store = Store::ephemeral();
    store.record(
        "6b4121313cc4a35d",
        Progress {
            title: "Legacy sample".to_string(),
            chapter: 2,
            para: 3,
            chars_read: 400,
            ..Progress::default()
        },
    );

    let progress = store
        .migrate_book_key("readio:sample:v1", &["6b4121313cc4a35d".to_string()])
        .expect("legacy progress");

    assert_eq!(progress.chapter, 2);
    assert_eq!(progress.para, 3);
    assert!(store.get("6b4121313cc4a35d").is_none());
    assert_eq!(
        store.get("readio:sample:v1").map(|item| item.chars_read),
        Some(400)
    );
}
