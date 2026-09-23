use readio_bridge::normalize_tts_catalog_json;
use serde_json::json;

#[test]
fn the_json_bridge_preserves_the_server_contract_while_rust_normalizes_it() {
    let raw = json!({
        "schemaVersion": 1,
        "defaultModelId": "missing",
        "models": [{
            "id": " workers-ai/melotts ",
            "provider": " Workers-AI ",
            "label": " MeloTTS ",
            "languages": ["ZH_hans_cn", "zh-Hans-CN"],
            "voices": [{"id": " default ", "label": " Default "}],
            "defaultVoiceId": "missing"
        }]
    });

    let normalized = normalize_tts_catalog_json(&raw.to_string()).expect("valid catalog");

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&normalized).expect("bridge JSON"),
        json!({
            "schemaVersion": 1,
            "defaultModelId": "workers-ai/melotts",
            "models": [{
                "id": "workers-ai/melotts",
                "provider": "workers-ai",
                "label": "MeloTTS",
                "languages": ["zh-Hans-CN"],
                "voices": [{"id": "default", "label": "Default"}],
                "defaultVoiceId": "default"
            }]
        }),
    );
}
