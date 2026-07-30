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
//! - [`mode`] / [`effort`] — the three ways a reader moves through a book, and
//!   the pace they read at, worn as an agent's reasoning effort.
//! - [`i18n`] — every user-visible string, Chinese and English.
//! - [`voice`] — read-aloud: which engine reads, and the playback behind it.

pub mod app;
pub mod book;
pub mod cli;
pub mod config;
pub mod effort;
pub mod i18n;
pub mod library;
pub mod metrics;
pub mod mode;
pub mod paths;
pub mod store;
pub mod stream;
pub mod theme;
pub mod ui;
pub mod util;
pub mod voice;
pub mod wrap;
