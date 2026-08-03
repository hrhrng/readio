//! Where readio keeps its things.
//!
//! Everything lives under one host directory, `~/.readio` by default. The
//! directory can be moved with `readio --home <dir>`, which is also what lets
//! the test suite run against a temp dir instead of a real library. readio
//! reads no environment variables: every setting lives in `config.yaml`.
//!
//! ```text
//! ~/.readio/
//!   config.yaml     every setting: language, speed, read-aloud, images
//!   library.json    imported books, in import order
//!   state.json      reading progress
//!   books/          copies of imported books (-c and -m)
//!   speech/         scratch audio clips, deleted as they play
//! ```
//!
//! Optional language runtimes are rebuildable rather than library data. They
//! live in the platform's user cache (`~/Library/Caches/readio` on macOS,
//! usually `~/.cache/readio` on Linux), beside the caches uv and Hugging Face
//! already share across applications.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};

/// Explicit host directory, from `--home` or a test harness.
static HOME: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Point readio at a different host directory. Call before anything is loaded.
pub fn set_home(dir: impl Into<PathBuf>) {
    if let Ok(mut slot) = HOME.write() {
        *slot = Some(dir.into());
    }
}

/// Root of the host directory.
pub fn home() -> PathBuf {
    if let Ok(slot) = HOME.read()
        && let Some(dir) = slot.as_ref()
    {
        return dir.clone();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".readio")
}

/// Directory holding imported book files.
pub fn books_dir() -> PathBuf {
    home().join("books")
}

pub fn state_file() -> PathBuf {
    home().join("state.json")
}

pub fn library_file() -> PathBuf {
    home().join("library.json")
}

/// The one configuration file.
pub fn config_file() -> PathBuf {
    home().join("config.yaml")
}

/// Cache of images extracted from books, keyed by book id.
pub fn images_dir() -> PathBuf {
    home().join("images")
}

/// Scratch directory for synthesized audio clips, cleared as clips are played.
pub fn speech_dir() -> PathBuf {
    home().join("speech")
}

/// Rebuildable content-addressed speech audio.
///
/// Unlike live scratch clips this belongs in the platform cache: deleting it
/// only costs synthesis time, and clearing `~/.readio` should remain about the
/// reader's library and choices rather than hundreds of megabytes of PCM.
pub fn speech_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| home().join("cache"))
        .join("readio/speech-v1")
}

/// Where readio writes the helper scripts it carries.
///
/// Beside the config rather than in a cache: it is a file the reader is allowed
/// to open and, if they like, change — the same bargain the config file offers.
pub fn engines_dir() -> PathBuf {
    home().join("engines")
}

/// Rebuildable, readio-owned runtimes for optional model engines.
pub fn runtime_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| home().join("cache"))
        .join("readio/runtime")
}

/// Model launchers installed by the managed runtime.
pub fn runtime_bin_dir() -> PathBuf {
    runtime_dir().join("bin")
}

/// Isolated environments created by `uv tool install`.
pub fn runtime_tools_dir() -> PathBuf {
    runtime_dir().join("tools")
}

/// Python builds downloaded by uv for model environments.
pub fn runtime_python_dir() -> PathBuf {
    runtime_dir().join("python")
}

/// Create the host directory tree if it does not exist yet.
pub fn ensure_dirs() -> Result<()> {
    let books = books_dir();
    std::fs::create_dir_all(&books)
        .with_context(|| format!("cannot create {}", books.display()))?;
    Ok(())
}

/// Write a file by way of a temp file and a rename, so a crash mid-write cannot
/// leave a truncated index behind.
///
/// The temp name carries the process id and a counter: two readio processes (or
/// two tests) saving at the same moment must not race on one scratch file.
pub fn write_atomic(path: &Path, body: &str) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    ensure_dirs()?;
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "readio".to_string());
    let tmp = path.with_file_name(format!(
        "{stem}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    std::fs::write(&tmp, body).with_context(|| format!("cannot write {}", tmp.display()))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err).with_context(|| format!("cannot update {}", path.display()))
        }
    }
}

/// Render a path for display, shortening the user's home to `~`.
pub fn display(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}
