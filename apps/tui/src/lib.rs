//! readio — a terminal reader that works like a coding agent.
//!
//! The disguise is the point: reading a book becomes a turn of an agent
//! session, complete with reasoning, tool calls against the book's own URI
//! space, and text that streams in at reading speed.
//!
//! Layers, bottom up:
//!
//! - [`wrap`] / [`stream`] — display-width text handling and phrase pacing.
//! - [`book`] — EPUB / text parsing into chapters and paragraphs.
//! - [`paths`] / [`library`] / [`store`] / [`config`] — the host directory:
//!   imported books, the library index, reading progress, and the one settings
//!   file. readio reads no environment variables.
//! - [`ui`] — blocks, the scrollback surface, the prompt, the chrome.
//! - [`app`] — the turn state machine and the reading logic that feeds it.
//! - [`i18n`] — every user-visible string, Chinese and English.
//! - [`tts`] — read-aloud: local speech engines and playback.

pub mod app;
pub mod book;
pub mod cli;
pub mod config;
pub mod i18n;
pub mod library;
pub mod metrics;
pub mod paths;
pub mod store;
pub mod stream;
pub mod theme;
pub mod tts;
pub mod ui;
pub mod util;
pub mod wrap;
