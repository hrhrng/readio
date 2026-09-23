use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SUPPORT_BUNDLE_SCHEMA_VERSION: u32 = 1;
pub const MAX_DIAGNOSTIC_EVENTS: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    Import,
    Parsing,
    Sync,
    Tts,
    Fallback,
    Playback,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiagnosticEventInput {
    pub timestamp_unix_ms: u64,
    pub operation_id: String,
    pub component: String,
    pub category: DiagnosticCategory,
    pub name: String,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SupportBundleRequest {
    pub generated_at_unix_ms: u64,
    pub platform: String,
    pub app_version: String,
    #[serde(default)]
    pub events: Vec<DiagnosticEventInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupportBundleRedaction {
    pub content_included: bool,
    pub secrets_included: bool,
    pub dropped_field_count: u64,
    pub dropped_event_count: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiagnosticEvent {
    pub timestamp_unix_ms: u64,
    pub operation_id: String,
    pub component: String,
    pub category: DiagnosticCategory,
    pub name: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SupportBundle {
    pub schema_version: u32,
    pub generated_at_unix_ms: u64,
    pub platform: String,
    pub app_version: String,
    pub redaction: SupportBundleRedaction,
    pub events: Vec<DiagnosticEvent>,
}

#[derive(Clone, Copy)]
enum FieldKind {
    Token,
    Number,
    Boolean,
}

/// Builds the only diagnostics payload that product surfaces may export by
/// default. The input is deliberately permissive so adapters can hand Rust
/// their local event dictionaries; the output is deliberately restrictive.
/// Unknown keys, content-shaped strings, URLs, credentials, and unbounded
/// history are removed here rather than trusting every UI to redact correctly.
pub fn build_support_bundle(request: SupportBundleRequest) -> SupportBundle {
    let dropped_event_count = request.events.len().saturating_sub(MAX_DIAGNOSTIC_EVENTS);
    let mut dropped_field_count = 0_u64;
    let start = dropped_event_count;
    let events = request
        .events
        .into_iter()
        .skip(start)
        .map(|event| sanitize_event(event, &mut dropped_field_count))
        .collect();

    SupportBundle {
        schema_version: SUPPORT_BUNDLE_SCHEMA_VERSION,
        generated_at_unix_ms: request.generated_at_unix_ms,
        platform: sanitize_token(&request.platform).unwrap_or_else(|| "unknown".to_string()),
        app_version: sanitize_token(&request.app_version)
            .unwrap_or_else(|| "unknown".to_string()),
        redaction: SupportBundleRedaction {
            content_included: false,
            secrets_included: false,
            dropped_field_count,
            dropped_event_count: dropped_event_count as u64,
        },
        events,
    }
}

fn sanitize_event(
    event: DiagnosticEventInput,
    dropped_field_count: &mut u64,
) -> DiagnosticEvent {
    let operation_id = sanitize_token(&event.operation_id).unwrap_or_else(|| {
        *dropped_field_count += 1;
        "redacted-operation".to_string()
    });
    let component = sanitize_token(&event.component).unwrap_or_else(|| {
        *dropped_field_count += 1;
        "redacted-component".to_string()
    });
    let name = sanitize_token(&event.name).unwrap_or_else(|| {
        *dropped_field_count += 1;
        "redacted-event".to_string()
    });
    let mut fields = BTreeMap::new();
    for (key, value) in event.fields {
        let Some(kind) = allowed_field(event.category, &key) else {
            *dropped_field_count += 1;
            continue;
        };
        let sanitized = match kind {
            FieldKind::Token => value
                .as_str()
                .and_then(sanitize_token)
                .map(Value::String),
            FieldKind::Number => value.as_number().and_then(|number| {
                let non_negative = number.as_u64().is_some()
                    || number.as_f64().is_some_and(|value| value.is_finite() && value >= 0.0);
                non_negative.then(|| Value::Number(number.clone()))
            }),
            FieldKind::Boolean => value.as_bool().map(Value::Bool),
        };
        if let Some(value) = sanitized {
            fields.insert(key, value);
        } else {
            *dropped_field_count += 1;
        }
    }

    DiagnosticEvent {
        timestamp_unix_ms: event.timestamp_unix_ms,
        operation_id,
        component,
        category: event.category,
        name,
        fields,
    }
}

fn allowed_field(category: DiagnosticCategory, key: &str) -> Option<FieldKind> {
    let common = match key {
        "phase" | "outcome" | "reason_code" => Some(FieldKind::Token),
        "duration_ms" | "status_code" => Some(FieldKind::Number),
        _ => None,
    };
    if common.is_some() {
        return common;
    }

    match category {
        DiagnosticCategory::Import => match key {
            "source_kind" | "document_format" => Some(FieldKind::Token),
            "byte_count" => Some(FieldKind::Number),
            "duplicate" => Some(FieldKind::Boolean),
            _ => None,
        },
        DiagnosticCategory::Parsing => match key {
            "document_format" => Some(FieldKind::Token),
            "byte_count" | "block_count" | "chapter_count" => Some(FieldKind::Number),
            _ => None,
        },
        DiagnosticCategory::Sync => match key {
            "direction" => Some(FieldKind::Token),
            "change_count" | "cursor" | "retry_count" => Some(FieldKind::Number),
            _ => None,
        },
        DiagnosticCategory::Tts => match key {
            "provider" | "trace_id" | "model_id" | "job_id" => Some(FieldKind::Token),
            "audio_duration_ms" | "retry_count" => Some(FieldKind::Number),
            "cache_hit" => Some(FieldKind::Boolean),
            _ => None,
        },
        DiagnosticCategory::Fallback => match key {
            "from_provider" | "to_provider" => Some(FieldKind::Token),
            "attempt" => Some(FieldKind::Number),
            _ => None,
        },
        DiagnosticCategory::Playback => match key {
            "mode" => Some(FieldKind::Token),
            "generation"
            | "sentence_index"
            | "media_position_ms"
            | "media_duration_ms"
            | "playback_rate_milli" => Some(FieldKind::Number),
            "playing" => Some(FieldKind::Boolean),
            _ => None,
        },
    }
}

fn sanitize_token(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 96 {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("bearer")
        || lower.starts_with("basic")
        || lower.starts_with("sk-")
        || lower.contains("://")
        || lower.contains('@')
        || (lower.starts_with("eyj") && lower.matches('.').count() >= 2)
    {
        return None;
    }
    value
        .chars()
        .all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '/')
        })
        .then(|| value.to_string())
}
