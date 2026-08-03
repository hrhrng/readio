//! Library tests: the three import modes, id stability, and removal.
//!
//! Every test runs against a temp `READIO_HOME`, so a test run never touches
//! the developer's real library.

mod common;

use std::path::PathBuf;

use common::{isolated_home, source_book, under_books};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::{App, parse_import_arg};
use readio::book::content_id;
use readio::library::{Library, Mode};
use readio::paths;
use readio::store::{Progress, SpeechCheckpoint, Store};

fn library() -> Library {
    isolated_home();
    // Persisting is fine — it writes inside the temp home.
    Library::default()
}

#[test]
fn copy_is_the_default_and_leaves_the_original_alone() {
    let source = source_book("copy-mode", "复制模式");
    let mut lib = library();

    let (entry, book) = lib.import(&source, Mode::Copy).expect("import");

    assert_eq!(entry.mode, Mode::Copy);
    assert_eq!(book.title, "复制模式");
    assert!(source.exists(), "the original must stay put");
    assert!(entry.path.exists(), "the copy must exist");
    assert!(
        under_books(&entry.path),
        "copy should live in the books dir, got {}",
        entry.path.display()
    );
    assert_ne!(entry.path, source, "copy is a different file");
    assert_eq!(entry.origin.as_deref(), Some(source.as_path()));
    assert_eq!(lib.len(), 1);
    assert_eq!(lib.get(1).map(|e| e.id.clone()), Some(entry.id));
}

#[test]
fn link_records_a_reference_without_copying() {
    let source = source_book("link-mode", "引用模式");
    let mut lib = library();

    let (entry, _) = lib.import(&source, Mode::Link).expect("import");

    assert_eq!(entry.mode, Mode::Link);
    assert_eq!(entry.path, source, "link points at the original file");
    assert!(entry.origin.is_none(), "no copy, so no separate origin");
    assert!(source.exists());
    let copies = std::fs::read_dir(paths::books_dir())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().contains("引用模式"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(copies, 0, "link mode must not copy anything");
}

#[test]
fn move_relocates_the_file() {
    let source = source_book("move-mode", "移动模式");
    let mut lib = library();

    let (entry, _) = lib.import(&source, Mode::Move).expect("import");

    assert_eq!(entry.mode, Mode::Move);
    assert!(!source.exists(), "the original should be gone");
    assert!(entry.path.exists(), "the file should be in the library");
    assert!(under_books(&entry.path));
}

#[test]
fn identity_follows_content_not_location() {
    let source = source_book("identity", "同一本书");
    let before = content_id(&source).expect("id");

    let mut lib = library();
    let (entry, _) = lib.import(&source, Mode::Copy).expect("import");
    let after = content_id(&entry.path).expect("id of the copy");

    assert_eq!(before, after, "copying must not change the id");
    assert_eq!(entry.id, before, "the entry is keyed by content");
}

#[test]
fn reimporting_the_same_book_from_another_path_does_not_duplicate() {
    let first = source_book("dup-a", "重复导入");
    let mut lib = library();
    let (entry_a, _) = lib.import(&first, Mode::Copy).expect("first import");

    // Same bytes, different path.
    let second = source_book("dup-b", "重复导入");
    let (entry_b, _) = lib.import(&second, Mode::Copy).expect("second import");

    assert_eq!(entry_a.id, entry_b.id, "same content, same id");
    assert_eq!(lib.len(), 1, "the library should hold one entry");
}

#[test]
fn changing_mode_on_reimport_switches_how_the_file_is_held() {
    let source = source_book("mode-switch", "切换模式");
    let mut lib = library();

    let (linked, _) = lib.import(&source, Mode::Link).expect("link");
    assert_eq!(linked.path, source);

    let (copied, _) = lib.import(&source, Mode::Copy).expect("copy");
    assert_eq!(lib.len(), 1, "still one entry");
    assert!(under_books(&copied.path));
    assert_eq!(copied.mode, Mode::Copy);
}

#[test]
fn forget_removes_the_entry_and_only_deletes_library_copies() {
    let copied_src = source_book("forget-copy", "要忘掉的复制本");
    let linked_src = source_book("forget-link", "要忘掉的引用");
    let mut lib = library();

    let (copied, _) = lib.import(&copied_src, Mode::Copy).expect("copy");
    let (linked, _) = lib.import(&linked_src, Mode::Link).expect("link");
    assert_eq!(lib.len(), 2);

    // Indices are one-based, matching the listing.
    let linked_index = 1 + lib
        .entries
        .iter()
        .position(|e| e.id == linked.id)
        .expect("linked entry");
    let removed = lib.forget(linked_index).expect("forget link");
    assert_eq!(removed.id, linked.id);
    assert!(
        linked_src.exists(),
        "a linked original must never be deleted"
    );
    assert_eq!(lib.len(), 1);

    lib.forget(1).expect("forget copy");
    assert!(!copied.path.exists(), "the library copy should be deleted");
    assert!(copied_src.exists(), "the user's own file is untouched");
    assert!(lib.is_empty());

    assert!(
        lib.forget(1).is_err(),
        "removing from an empty library fails"
    );
}

#[test]
fn forget_removes_saved_progress_and_the_resume_pointer() {
    let source = source_book("forget-state", "忘干净的书");
    let mut lib = library();
    let (entry, book) = lib.import(&source, Mode::Copy).expect("import");
    let mut store = Store::default();
    store.record(
        &entry.id,
        Progress {
            title: entry.title.clone(),
            path: Some(entry.path.to_string_lossy().into_owned()),
            speech: Some(SpeechCheckpoint {
                chapter: 0,
                para: 0,
                from: 3,
                anchor: "旧朗读断点".to_string(),
            }),
            ..Progress::default()
        },
    );
    store.save().expect("save progress");
    assert_eq!(store.last_book.as_deref(), entry.path.to_str());

    let id = entry.id.clone();
    let mut app = App::new(lib, store, Some(book), None);
    app.on_paste("/forget 1".to_string());
    app.on_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.library.is_empty(), "the library entry should be gone");
    assert!(
        app.store.get(&id).is_none(),
        "forget must remove the saved position and speech checkpoint"
    );
    assert_eq!(
        app.store.last_book, None,
        "a forgotten book cannot remain the bare-launch resume target"
    );

    let reloaded = Store::load();
    assert!(
        reloaded.get(&id).is_none(),
        "the removal must survive a restart"
    );
    assert_eq!(reloaded.last_book, None);
}

#[test]
fn refuses_what_it_cannot_read() {
    isolated_home();
    let dir = isolated_home().join("sources").join("bad");
    std::fs::create_dir_all(&dir).expect("dir");
    let odd = dir.join("notes.docx");
    std::fs::write(&odd, b"PK\x03\x04").expect("write");

    let mut lib = library();
    let err = lib
        .import(&odd, Mode::Copy)
        .expect_err("docx is unsupported");
    assert!(format!("{err}").contains("不支持"), "got: {err}");

    // A supported extension still has to hold something readable.
    let broken = dir.join("notes.pdf");
    std::fs::write(&broken, b"%PDF-1.4").expect("write");
    assert!(
        lib.import(&broken, Mode::Copy).is_err(),
        "a truncated pdf is not readable"
    );

    let missing = dir.join("nope.epub");
    assert!(lib.import(&missing, Mode::Copy).is_err(), "missing file");
    assert!(lib.import(&dir, Mode::Copy).is_err(), "a directory");
    assert!(lib.is_empty(), "nothing should have been recorded");
}

#[test]
fn the_index_survives_a_round_trip_through_disk() {
    let source = source_book("persisted", "会存盘的书");
    let mut lib = library();
    let (entry, _) = lib.import(&source, Mode::Copy).expect("import");
    lib.save().expect("save");

    let reloaded = Library::load();
    let found = reloaded.find(&entry.id).expect("entry should persist");
    assert_eq!(found.title, "会存盘的书");
    assert_eq!(found.mode, Mode::Copy);
    assert!(found.chars > 0 && found.chapters >= 1);
}

#[test]
fn import_arguments_accept_a_flag_on_either_side() {
    assert_eq!(
        parse_import_arg("book.epub -l"),
        (PathBuf::from("book.epub"), Mode::Link)
    );
    assert_eq!(
        parse_import_arg("-m book.epub"),
        (PathBuf::from("book.epub"), Mode::Move)
    );
    assert_eq!(
        parse_import_arg("book.epub"),
        (PathBuf::from("book.epub"), Mode::Copy),
        "copy is the default"
    );
    assert_eq!(
        parse_import_arg("my notes.md"),
        (PathBuf::from("my notes.md"), Mode::Copy),
        "paths with spaces should survive"
    );
}
