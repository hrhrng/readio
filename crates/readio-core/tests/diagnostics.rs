use readio_core::diagnostics::{
    build_support_bundle, DiagnosticCategory, DiagnosticEventInput, SupportBundleRequest,
    MAX_DIAGNOSTIC_EVENTS,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fields(entries: &[(&str, Value)]) -> BTreeMap<String, Value> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

#[test]
fn support_bundle_correlates_events_without_exporting_content_or_secrets() {
    let operation_id = "op-018f7f22-43d0-7cc8-bd1a-09d9257d3b8e";
    let bundle = build_support_bundle(SupportBundleRequest {
        generated_at_unix_ms: 1_754_815_000_000,
        platform: "web".to_string(),
        app_version: "0.2.0-beta.12".to_string(),
        events: vec![
            DiagnosticEventInput {
                timestamp_unix_ms: 1_754_814_999_000,
                operation_id: operation_id.to_string(),
                component: "web-reader".to_string(),
                category: DiagnosticCategory::Tts,
                name: "provider_completed".to_string(),
                fields: fields(&[
                    ("provider", json!("workers-ai")),
                    ("trace_id", json!("trace-42")),
                    ("duration_ms", json!(842)),
                    ("input_text", json!("Private sentence from the book.")),
                    ("authorization", json!("Bearer super-secret")),
                    ("api_key", json!("sk-live-secret")),
                ]),
            },
            DiagnosticEventInput {
                timestamp_unix_ms: 1_754_814_999_500,
                operation_id: operation_id.to_string(),
                component: "cloud-worker".to_string(),
                category: DiagnosticCategory::Playback,
                name: "clock_sample".to_string(),
                fields: fields(&[
                    ("generation", json!(7)),
                    ("sentence_index", json!(12)),
                    ("media_position_ms", json!(1_250)),
                    ("media_duration_ms", json!(4_000)),
                    ("book_text", json!("Private sentence from the book.")),
                    ("url", json!("https://user:pass@example.test/private")),
                ]),
            },
        ],
    });

    assert_eq!(bundle.schema_version, 1);
    assert!(!bundle.redaction.content_included);
    assert!(!bundle.redaction.secrets_included);
    assert!(bundle.redaction.dropped_field_count >= 5);
    assert_eq!(bundle.events.len(), 2);
    assert!(
        bundle
            .events
            .iter()
            .all(|event| event.operation_id == operation_id)
    );
    assert_eq!(bundle.events[0].fields["provider"], json!("workers-ai"));
    assert_eq!(bundle.events[0].fields["trace_id"], json!("trace-42"));
    assert_eq!(
        bundle.events[1].fields["media_position_ms"],
        json!(1_250)
    );

    let exported = serde_json::to_string(&bundle).expect("support bundle JSON");
    for forbidden in [
        "Private sentence",
        "super-secret",
        "sk-live-secret",
        "user:pass",
        "input_text",
        "authorization",
        "api_key",
        "book_text",
    ] {
        assert!(
            !exported.contains(forbidden),
            "support bundle leaked {forbidden}: {exported}"
        );
    }
}

#[test]
fn support_bundle_keeps_only_the_latest_bounded_event_window() {
    let events = (0..MAX_DIAGNOSTIC_EVENTS + 3)
        .map(|index| DiagnosticEventInput {
            timestamp_unix_ms: index as u64,
            operation_id: format!("op-{index}"),
            component: "tui".to_string(),
            category: DiagnosticCategory::Sync,
            name: "pull_completed".to_string(),
            fields: fields(&[("change_count", json!(index))]),
        })
        .collect();

    let bundle = build_support_bundle(SupportBundleRequest {
        generated_at_unix_ms: 999,
        platform: "tui".to_string(),
        app_version: "0.2.0-beta.12".to_string(),
        events,
    });

    assert_eq!(bundle.events.len(), MAX_DIAGNOSTIC_EVENTS);
    assert_eq!(bundle.events[0].operation_id, "op-3");
    assert_eq!(
        bundle.redaction.dropped_event_count,
        3,
        "the bundle must report truncation instead of hiding it"
    );
}
