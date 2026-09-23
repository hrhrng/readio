//! The library: every book that has been imported into the host directory.
//!
//! Importing is explicit and has three shapes, chosen at the command line:
//!
//! | flag | meaning |
//! | --- | --- |
//! | `-c` | copy the file into `~/.readio/books` (the default) |
//! | `-l` | link: leave the file where it is and only record a reference |
//! | `-m` | move the file into `~/.readio/books`, removing the original |
//!
//! Entries are keyed by [`crate::book::content_id`], so re-importing the same
//! book — in any mode, from any path — updates the existing entry instead of
//! creating a duplicate, and keeps its reading progress.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use readio_sdk::{
    ReadioSdk,
    imports::{ImportEffect, ImportOutcome, ImportOwnership, ImportRecord, ImportRequest},
};
use serde::{Deserialize, Serialize};

use crate::book::{Book, content_id, slug};
use crate::i18n::tf;
use crate::paths;
use crate::store::now_secs;

/// How an imported file is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Copy into the host directory. The default.
    #[default]
    Copy,
    Link,
    Move,
}

impl Mode {
    /// Parse a command-line flag. `None` for anything unrecognised.
    pub fn parse(flag: &str) -> Option<Self> {
        match flag {
            "-c" | "--copy" => Some(Mode::Copy),
            "-l" | "--link" => Some(Mode::Link),
            "-m" | "--move" => Some(Mode::Move),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Copy => "copy",
            Mode::Link => "link",
            Mode::Move => "move",
        }
    }

    /// One-line description used in notices, in the interface language.
    pub fn describe(self) -> &'static str {
        match self {
            Mode::Copy => crate::i18n::t("lib.mode_copy"),
            Mode::Link => crate::i18n::t("lib.mode_link"),
            Mode::Move => crate::i18n::t("lib.mode_move"),
        }
    }

    fn ownership(self) -> ImportOwnership {
        match self {
            Mode::Copy => ImportOwnership::Copy,
            Mode::Link => ImportOwnership::Link,
            Mode::Move => ImportOwnership::Move,
        }
    }
}

/// One imported book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Content id, shared with the progress store.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    /// Where the file readio reads actually lives.
    pub path: PathBuf,
    /// Where it was imported from, when that differs.
    #[serde(default)]
    pub origin: Option<PathBuf>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub import_record: Option<ImportRecord>,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub chars: usize,
    /// Words, counted per script at import time. Zero on records written before
    /// readio counted them; [`Entry::amount`] falls back to an estimate, and
    /// opening the book fills it in.
    #[serde(default)]
    pub words: usize,
    #[serde(default)]
    pub chapters: usize,
    #[serde(default)]
    pub imported: u64,
    #[serde(default)]
    pub last_opened: u64,
}

impl Entry {
    /// True when the file behind this entry is still readable.
    pub fn available(&self) -> bool {
        self.path.exists()
    }

    /// How long this book is, in the reader's own unit.
    pub fn amount(&self) -> String {
        let words = if self.words > 0 {
            self.words
        } else {
            crate::metrics::words_from_char_count(self.chars)
        };
        crate::metrics::amount(self.chars, words)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// When false, [`Library::save`] is a no-op (used by tests).
    #[serde(skip, default = "enabled")]
    pub persist: bool,
}

fn enabled() -> bool {
    true
}

impl Default for Library {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            persist: true,
        }
    }
}

impl Library {
    /// Load the index, treating corruption as an empty library.
    pub fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(paths::library_file()) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    /// An in-memory library that never writes to disk.
    pub fn ephemeral() -> Self {
        Self {
            persist: false,
            ..Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        if !self.persist {
            return Ok(());
        }
        paths::write_atomic(&paths::library_file(), &serde_json::to_string_pretty(self)?)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// One-based lookup, matching what the listing shows.
    pub fn get(&self, index: usize) -> Option<&Entry> {
        index
            .checked_sub(1)
            .and_then(|zero_based| self.entries.get(zero_based))
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Entry most recently opened, offered on a bare launch.
    pub fn most_recent(&self) -> Option<&Entry> {
        self.entries
            .iter()
            .filter(|e| e.last_opened > 0)
            .max_by_key(|e| e.last_opened)
    }

    pub fn touch(&mut self, id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.last_opened = now_secs();
        }
        let _ = self.save();
    }

    /// Import `source` into the library and return the entry plus the parsed
    /// book, so the caller does not have to read the file twice.
    pub fn import(&mut self, source: &Path, mode: Mode) -> Result<(Entry, Book)> {
        let source = std::fs::canonicalize(source)
            .with_context(|| tf("lib.not_found", &[&source.display()]))?;
        if source.is_dir() {
            return Err(anyhow!("{}", tf("lib.is_dir", &[&paths::display(&source)])));
        }
        if !Book::supports(&source) {
            return Err(anyhow!(
                "{}",
                tf("lib.unsupported", &[&paths::display(&source)])
            ));
        }

        let source_bytes =
            std::fs::read(&source).with_context(|| tf("lib.not_found", &[&source.display()]))?;
        let sdk = ReadioSdk::new();
        let identity = sdk.imports.content_identity(&source_bytes);
        let shared_id = identity.digest.clone();
        let existing = self
            .entries
            .iter()
            .find(|entry| {
                entry.id == shared_id
                    || entry
                        .import_record
                        .as_ref()
                        .is_some_and(|record| record.content_identity == identity)
                    || (entry.available()
                        && content_id(&entry.path).is_ok_and(|identity| identity == shared_id))
            })
            .cloned();
        // Old releases keyed progress by a 16-character FNV prefix. Once an
        // existing file proves it has the same shared identity, retain that
        // primary key so state.json and bookmarks remain reachable.
        let id = existing
            .as_ref()
            .map_or_else(|| shared_id.clone(), |entry| entry.id.clone());
        let mut book = Book::load(Some(&source))?;
        book.id = id.clone();
        let bytes = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);

        let owned_storage = self.destination(&id, &book, &source);
        let source_key = source.to_string_lossy().into_owned();
        let existing_record = existing.as_ref().map(|entry| {
            entry.import_record.clone().unwrap_or_else(|| ImportRecord {
                item_id: entry.id.clone(),
                content_identity: identity.clone(),
                ownership: entry.mode.ownership(),
                source_key: entry
                    .origin
                    .as_ref()
                    .unwrap_or(&entry.path)
                    .to_string_lossy()
                    .into_owned(),
                storage_key: entry.path.to_string_lossy().into_owned(),
            })
        });
        let plan = sdk.imports.plan(ImportRequest {
            content_identity: identity,
            ownership: mode.ownership(),
            source_key,
            owned_storage_key: owned_storage.to_string_lossy().into_owned(),
            replace_ownership: existing.as_ref().is_some_and(|entry| entry.mode != mode),
            existing: existing_record,
        });
        if plan.outcome == ImportOutcome::Reused
            && let Some(existing) = existing.as_ref()
            && existing.available()
        {
            book.path = Some(existing.path.clone());
            return Ok((existing.clone(), book));
        }

        execute_import_effects(&plan.effects, &plan.record)?;
        let stored = PathBuf::from(&plan.record.storage_key);

        book.path = Some(stored.clone());

        let entry = Entry {
            id: id.clone(),
            title: book.title.clone(),
            author: book.author.clone(),
            path: stored,
            origin: (mode != Mode::Link).then(|| source.clone()),
            mode,
            import_record: Some(plan.record),
            bytes,
            chars: book.char_count(),
            words: book.word_count(),
            chapters: book.chapters.len(),
            imported: now_secs(),
            last_opened: existing.as_ref().map_or(0, |entry| entry.last_opened),
        };

        match self.entries.iter_mut().find(|e| e.id == id) {
            Some(slot) => {
                // Preserve the original import time on a re-import.
                let imported = slot.imported.min(entry.imported).max(1);
                *slot = Entry {
                    imported,
                    ..entry.clone()
                };
            }
            None => self.entries.push(entry.clone()),
        }
        self.save()?;
        Ok((entry, book))
    }

    /// `~/.readio/books/<id>-<slug>.<ext>`, with the id keeping names unique.
    fn destination(&self, id: &str, book: &Book, source: &Path) -> PathBuf {
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt")
            .to_ascii_lowercase();
        let name = slug(&book.title);
        paths::books_dir().join(format!("{}-{}.{}", &id[..8], name, ext))
    }

    /// Forget an entry. Files copied into the library are removed too; a linked
    /// file is left alone, since it belongs to the user's own tree.
    pub fn forget(&mut self, index: usize) -> Result<Entry> {
        let zero = index
            .checked_sub(1)
            .filter(|i| *i < self.entries.len())
            .ok_or_else(|| anyhow!("{}", tf("lib.no_entry", &[&index])))?;
        let entry = self.entries.remove(zero);
        if let Some(record) = &entry.import_record {
            for effect in ReadioSdk::new().imports.forget(record) {
                if let ImportEffect::DeleteOwnedContent { storage_key } = effect {
                    delete_owned_content(Path::new(&storage_key));
                }
            }
        } else if entry.mode != Mode::Link {
            delete_owned_content(&entry.path);
        }
        self.save()?;
        Ok(entry)
    }
}

fn execute_import_effects(effects: &[ImportEffect], record: &ImportRecord) -> Result<()> {
    for effect in effects {
        match effect {
            ImportEffect::WriteOwnedContent {
                source_key,
                storage_key,
            } => {
                let source = Path::new(source_key);
                let storage = Path::new(storage_key);
                if source == storage {
                    continue;
                }
                if let Some(parent) = storage.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| tf("lib.write_failed", &[&paths::display(storage)]))?;
                }
                std::fs::copy(source, storage)
                    .with_context(|| tf("lib.copy_failed", &[&paths::display(storage)]))?;
            }
            ImportEffect::DeleteSourceAfterCommit { source_key } => {
                let source = Path::new(source_key);
                if source != Path::new(&record.storage_key) {
                    std::fs::remove_file(source)
                        .with_context(|| tf("lib.moved_but_kept", &[&paths::display(source)]))?;
                }
            }
            ImportEffect::DeleteOwnedContent { storage_key } => {
                delete_owned_content(Path::new(storage_key));
            }
        }
    }
    Ok(())
}

fn delete_owned_content(path: &Path) {
    if path.starts_with(paths::books_dir()) {
        let _ = std::fs::remove_file(path);
    }
}
