//! Reading-progress persistence: `~/.readio/state.json`.
//!
//! State, not settings: where the reader is in each book. Everything they chose
//! lives in [`crate::config`].
//!
//! Writes go through a temp file + rename so a crash mid-save cannot leave a
//! truncated state file behind.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use readio_sdk::position::TextLocator;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub title: String,
    #[serde(default)]
    pub path: Option<String>,
    pub chapter: usize,
    pub para: usize,
    #[serde(default)]
    pub chars_read: u64,
    #[serde(default)]
    pub sessions: u32,
    #[serde(default)]
    pub updated: u64,
    /// Renderer-independent place in the canonical Rust Document IR. Older
    /// state files keep using `chars_read`; new writes carry both so a parser
    /// revision or an inserted block can recover the same reading intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<TextLocator>,
    /// Fine-grained read-aloud position inside the passage that starts at
    /// `(chapter, para)`. Ordinary reading still commits whole paragraph
    /// windows; speech additionally keeps the current sentence so a process
    /// restart does not replay that window from its first word.
    #[serde(default)]
    pub speech: Option<SpeechCheckpoint>,
    /// Places the reader asked to keep. Empty for a book nobody marked, and
    /// absent from an older state file, which is what `default` is for.
    #[serde(default)]
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechCheckpoint {
    pub chapter: usize,
    pub para: usize,
    /// Byte offset in the rendered passage. Sentence and synthesis ranges use
    /// bytes too, so resuming does not round through a second coordinate system.
    pub from: usize,
    /// Opening text at `from`, used to relocate the sentence if passage packing
    /// changes between releases.
    #[serde(default)]
    pub anchor: String,
}

/// A place worth coming back to.
///
/// `chars` is the coordinate that matters: chapter and paragraph indices are a
/// convenience for showing where a mark is, but they are relative to how the
/// book was cut into chapters when the mark was made, and that can change.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mark {
    pub chars: u64,
    #[serde(default)]
    pub chapter: usize,
    #[serde(default)]
    pub para: usize,
    /// What the reader called it, or the opening words of the passage when they
    /// did not say.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub books: HashMap<String, Progress>,
    /// Most recently opened book, offered on a bare launch.
    #[serde(default)]
    pub last_book: Option<String>,
    /// When false, [`Store::save`] is a no-op. Tests use this so a test run
    /// cannot clobber the developer's real reading progress.
    #[serde(skip, default = "enabled")]
    pub persist: bool,
}

fn enabled() -> bool {
    true
}

impl Default for Store {
    fn default() -> Self {
        Self {
            books: HashMap::new(),
            last_book: None,
            persist: true,
        }
    }
}

impl Store {
    /// An in-memory store that never touches the filesystem.
    pub fn ephemeral() -> Self {
        Self {
            persist: false,
            ..Self::default()
        }
    }

    pub fn path() -> PathBuf {
        crate::paths::state_file()
    }

    /// Load state, treating any corruption as "no history yet".
    pub fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::path()) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        if !self.persist {
            return Ok(());
        }
        crate::paths::write_atomic(&Self::path(), &serde_json::to_string_pretty(self)?)
    }

    pub fn get(&self, id: &str) -> Option<&Progress> {
        self.books.get(id)
    }

    /// Moves a record written under an older identity to its canonical key.
    /// Existing canonical progress wins; aliases are removed either way so the
    /// next sync cannot publish two positions for the same virtual document.
    pub fn migrate_book_key(
        &mut self,
        canonical_id: &str,
        legacy_ids: &[String],
    ) -> Option<Progress> {
        if let Some(progress) = self.books.get(canonical_id).cloned() {
            for legacy_id in legacy_ids {
                self.books.remove(legacy_id);
            }
            return Some(progress);
        }

        let progress = legacy_ids
            .iter()
            .find_map(|legacy_id| self.books.remove(legacy_id));
        for legacy_id in legacy_ids {
            self.books.remove(legacy_id);
        }
        if let Some(progress) = progress.as_ref() {
            self.books
                .insert(canonical_id.to_string(), progress.clone());
        }
        progress
    }

    pub fn record(&mut self, id: &str, progress: Progress) {
        self.last_book = progress.path.clone().or(self.last_book.clone());
        self.books.insert(id.to_string(), progress);
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
