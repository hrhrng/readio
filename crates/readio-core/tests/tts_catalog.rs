use readio_core::tts::{TtsModel, TtsModelCatalog, TtsVoice, normalize_tts_catalog};

#[test]
fn the_client_normalizes_the_server_catalog_without_reordering_it() {
    let catalog = TtsModelCatalog {
        schema_version: 1,
        default_model_id: " missing ".to_string(),
        models: vec![
            TtsModel {
                id: " workers-ai/aura-2-en ".to_string(),
                provider: " Workers-AI ".to_string(),
                label: " Aura 2 English ".to_string(),
                languages: vec!["EN_us".to_string(), "en-US".to_string(), String::new()],
                voices: vec![
                    TtsVoice {
                        id: " luna ".to_string(),
                        label: " Luna ".to_string(),
                    },
                    TtsVoice {
                        id: "luna".to_string(),
                        label: "Duplicate".to_string(),
                    },
                ],
                default_voice_id: "missing".to_string(),
            },
            TtsModel {
                id: "workers-ai/aura-2-en".to_string(),
                provider: "workers-ai".to_string(),
                label: "Duplicate model".to_string(),
                languages: vec!["en".to_string()],
                voices: vec![TtsVoice {
                    id: "other".to_string(),
                    label: "Other".to_string(),
                }],
                default_voice_id: "other".to_string(),
            },
            TtsModel {
                id: String::new(),
                provider: "workers-ai".to_string(),
                label: "Invalid".to_string(),
                languages: vec!["en".to_string()],
                voices: vec![],
                default_voice_id: String::new(),
            },
        ],
    };

    let normalized = normalize_tts_catalog(catalog).expect("a usable model remains");

    assert_eq!(normalized.default_model_id, "workers-ai/aura-2-en");
    assert_eq!(normalized.models.len(), 1);
    assert_eq!(normalized.models[0].id, "workers-ai/aura-2-en");
    assert_eq!(normalized.models[0].provider, "workers-ai");
    assert_eq!(normalized.models[0].label, "Aura 2 English");
    assert_eq!(normalized.models[0].languages, ["en-US"]);
    assert_eq!(normalized.models[0].voices.len(), 1);
    assert_eq!(normalized.models[0].voices[0].id, "luna");
    assert_eq!(normalized.models[0].voices[0].label, "Luna");
    assert_eq!(normalized.models[0].default_voice_id, "luna");
}
