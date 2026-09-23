//! Bounded, content-free product diagnostics for the terminal client.
//!
//! The terminal owns collection and file I/O. Rust Core still owns the export
//! policy: every bundle passes through its strict field allowlist before the
//! user can share it.

use std::collections::{BTreeMap, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use readio_sdk::ReadioSdk;
use readio_sdk::diagnostics::{
    DiagnosticCategory, DiagnosticEventInput, SupportBundle, SupportBundleRequest,
};
use serde_json::Value;

const DEFAULT_CAPACITY: usize = 768;
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static SUPPORT_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct DiagnosticLog {
    capacity: usize,
    events: VecDeque<DiagnosticEventInput>,
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl DiagnosticLog {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            events: VecDeque::new(),
        }
    }

    pub fn begin_operation(&self) -> String {
        let sequence = OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        format!("op-tui-{}-{sequence}", unix_time_milliseconds())
    }

    pub fn record(&mut self, event: DiagnosticEventInput) {
        self.events.push_back(event);
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
    }

    pub fn record_fact(
        &mut self,
        operation_id: impl Into<String>,
        component: impl Into<String>,
        category: DiagnosticCategory,
        name: impl Into<String>,
        fields: BTreeMap<String, Value>,
    ) {
        self.record(DiagnosticEventInput {
            timestamp_unix_ms: unix_time_milliseconds(),
            operation_id: operation_id.into(),
            component: component.into(),
            category,
            name: name.into(),
            fields,
        });
    }

    /// Restores only facts that have already passed through Rust Core's export
    /// allowlist. This lets `readio cloud sync` hand its short-lived operation
    /// to the next interactive `/diagnostics` export without persisting a raw
    /// trace or creating a second redaction policy in the TUI.
    pub fn load_persisted() -> Result<Self> {
        let path = crate::paths::diagnostics_file();
        let encoded = match std::fs::read_to_string(&path) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        let bundle: SupportBundle = serde_json::from_str(&encoded)
            .with_context(|| format!("cannot decode {}", path.display()))?;
        let mut log = Self::default();
        for event in bundle.events {
            log.record(DiagnosticEventInput {
                timestamp_unix_ms: event.timestamp_unix_ms,
                operation_id: event.operation_id,
                component: event.component,
                category: event.category,
                name: event.name,
                fields: event.fields,
            });
        }
        Ok(log)
    }

    /// Persists the bounded, Rust-sanitized bundle rather than the permissive
    /// event inputs held in memory.
    pub fn persist_safe(&self) -> Result<()> {
        let path = crate::paths::diagnostics_file();
        let bundle = self.build_support_bundle(unix_time_milliseconds());
        let mut encoded =
            serde_json::to_string_pretty(&bundle).context("cannot encode persisted diagnostics")?;
        encoded.push('\n');
        crate::paths::write_atomic(&path, &encoded)
    }

    pub fn build_support_bundle(&self, generated_at_unix_ms: u64) -> SupportBundle {
        ReadioSdk::new().diagnostics.build(SupportBundleRequest {
            generated_at_unix_ms,
            platform: "tui".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            events: self.events.iter().cloned().collect(),
        })
    }

    pub fn write_support_bundle(
        &self,
        path: &Path,
        generated_at_unix_ms: u64,
    ) -> Result<SupportBundle> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let bundle = self.build_support_bundle(generated_at_unix_ms);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("cannot create {}", path.display()))?;
        serde_json::to_writer_pretty(&mut file, &bundle)
            .with_context(|| format!("cannot write {}", path.display()))?;
        writeln!(file).with_context(|| format!("cannot finish {}", path.display()))?;
        Ok(bundle)
    }
}

pub fn unix_time_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub fn support_bundle_path(generated_at_unix_ms: u64) -> std::path::PathBuf {
    let sequence = SUPPORT_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    crate::paths::home().join("support").join(format!(
        "readio-support-{generated_at_unix_ms}-{sequence}.json"
    ))
}
