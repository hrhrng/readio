use std::path::PathBuf;

use readio::cloud::local_sync_payloads;
use readio::config::Config;
use readio::library::{Entry, Library, Mode};
use readio::store::{Progress, Store};

#[test]
fn cloud_payloads_never_expose_local_paths_or_engine_commands() {
    let library = Library {
        entries: vec![Entry {
            id: "book-1".to_string(),
            title: "Book".to_string(),
            author: None,
            path: PathBuf::from("/Users/reader/secret/book.epub"),
            origin: Some(PathBuf::from("/Volumes/private/book.epub")),
            mode: Mode::Link,
            import_record: None,
            bytes: 10,
            chars: 20,
            words: 4,
            chapters: 1,
            imported: 1,
            last_opened: 2,
        }],
        persist: false,
    };
    let mut store = Store::ephemeral();
    store.record(
        "book-1",
        Progress {
            title: "Book".to_string(),
            path: Some("/Users/reader/secret/book.epub".to_string()),
            chapter: 0,
            para: 2,
            chars_read: 10,
            sessions: 1,
            updated: 3,
            locator: None,
            speech: None,
            marks: vec![],
        },
    );
    let config = Config::default();

    let encoded = serde_json::to_string(&local_sync_payloads(&library, &store, &config))
        .expect("payload JSON");

    assert!(!encoded.contains("/Users/reader"));
    assert!(!encoded.contains("/Volumes/private"));
    assert!(!encoded.contains("engines"));
    assert!(encoded.contains("reading_position"));
    assert!(encoded.contains("library_item"));
}
