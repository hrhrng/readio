use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoice {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsModel {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub languages: Vec<String>,
    pub voices: Vec<TtsVoice>,
    pub default_voice_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsModelCatalog {
    pub schema_version: u32,
    pub default_model_id: String,
    pub models: Vec<TtsModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtsCatalogError {
    UnsupportedSchema(u32),
    EmptyCatalog,
}

impl std::fmt::Display for TtsCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported TTS model catalog schema {version}")
            }
            Self::EmptyCatalog => formatter.write_str("TTS model catalog has no usable models"),
        }
    }
}

impl std::error::Error for TtsCatalogError {}

pub fn normalize_tts_catalog(catalog: TtsModelCatalog) -> Result<TtsModelCatalog, TtsCatalogError> {
    if catalog.schema_version != 1 {
        return Err(TtsCatalogError::UnsupportedSchema(catalog.schema_version));
    }

    let mut seen_models = HashSet::new();
    let mut models = Vec::new();
    for model in catalog.models {
        let id = model.id.trim().to_string();
        let provider = model.provider.trim().to_ascii_lowercase();
        let label = model.label.trim().to_string();
        if id.is_empty()
            || provider.is_empty()
            || label.is_empty()
            || !seen_models.insert(id.clone())
        {
            continue;
        }

        let mut seen_languages = HashSet::new();
        let mut languages = model
            .languages
            .iter()
            .filter_map(|language| canonical_language_tag(language))
            .filter(|language| seen_languages.insert(language.clone()))
            .collect::<Vec<_>>();
        if languages.is_empty() {
            languages.push("und".to_string());
        }

        let mut seen_voices = HashSet::new();
        let voices = model
            .voices
            .into_iter()
            .filter_map(|voice| {
                let id = voice.id.trim().to_string();
                let label = voice.label.trim().to_string();
                if id.is_empty() || label.is_empty() || !seen_voices.insert(id.clone()) {
                    return None;
                }
                Some(TtsVoice { id, label })
            })
            .collect::<Vec<_>>();
        if voices.is_empty() {
            continue;
        }

        let requested_default = model.default_voice_id.trim();
        let default_voice_id = voices
            .iter()
            .find(|voice| voice.id == requested_default)
            .unwrap_or(&voices[0])
            .id
            .clone();
        models.push(TtsModel {
            id,
            provider,
            label,
            languages,
            voices,
            default_voice_id,
        });
    }

    if models.is_empty() {
        return Err(TtsCatalogError::EmptyCatalog);
    }
    let requested_default = catalog.default_model_id.trim();
    let default_model_id = models
        .iter()
        .find(|model| model.id == requested_default)
        .unwrap_or(&models[0])
        .id
        .clone();
    Ok(TtsModelCatalog {
        schema_version: 1,
        default_model_id,
        models,
    })
}

fn canonical_language_tag(value: &str) -> Option<String> {
    let normalized = value.trim().replace('_', "-");
    if normalized == "*" {
        return Some(normalized);
    }
    let parts = normalized.split('-').collect::<Vec<_>>();
    let primary = parts.first()?;
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return None;
    }

    let mut canonical = vec![primary.to_ascii_lowercase()];
    for part in parts.iter().skip(1) {
        if part.is_empty()
            || part.len() > 8
            || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return None;
        }
        canonical.push(
            if part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                let mut characters = part.chars();
                let first = characters.next()?.to_ascii_uppercase();
                format!("{first}{}", characters.as_str().to_ascii_lowercase())
            } else if (part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_alphabetic()))
                || (part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                part.to_ascii_uppercase()
            } else {
                part.to_ascii_lowercase()
            },
        );
    }
    Some(canonical.join("-"))
}
