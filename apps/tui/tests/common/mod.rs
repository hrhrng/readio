//! Shared test helpers.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use readio::paths;

/// Point readio's host directory at a temp directory, once per test binary.
///
/// All tests in a binary share it, so each still gets its own source files and
/// its own in-memory index. The language is pinned too: the rendering tests
/// assert on visible strings.
pub fn isolated_home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("readio-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp home");
        paths::set_home(&dir);
        readio::i18n::set(readio::i18n::Lang::Zh);
        dir
    })
    .as_path()
}

/// Write a small markdown book into its own source directory and return the
/// resolved path (macOS spells one directory both /var and /private/var).
#[allow(dead_code)]
pub fn source_book(name: &str, title: &str) -> PathBuf {
    let dir = isolated_home().join("sources").join(name);
    std::fs::create_dir_all(&dir).expect("source dir");
    let path = dir.join(format!("{name}.md"));
    std::fs::write(
        &path,
        format!("# {title}\n\n第一段内容，足够长到能读。\n\n## 第二节\n\n第二段内容。\n"),
    )
    .expect("write source");
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Whether `path` sits inside the library's books directory.
#[allow(dead_code, reason = "used by tests/library.rs only")]
pub fn under_books(path: &Path) -> bool {
    let books = paths::books_dir();
    let _ = std::fs::create_dir_all(&books);
    let resolve = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    resolve(path).starts_with(resolve(&books))
}
