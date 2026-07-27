//! Engine definitions and the built-in presets.
//!
//! The settings that select among these live in [`crate::config`]
//! (`~/.readio/config.yaml`); this module only describes how to drive an engine.
//!
//! readio ships presets for small local models rather than a bundled model:
//! the binary stays ~2 MB, and swapping in whatever is state of the art next
//! month is a one-line edit instead of a release. Every preset is just a
//! command template, so an engine readio has never heard of works too.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How to drive one speech engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSpec {
    /// Command that writes audio. Placeholders: `{text}` `{out}` `{voice}`
    /// `{rate}` `{model}` `{extra}`.
    pub synth: String,
    /// Command that plays a file, `{file}` being the clip. Empty means the
    /// synth command played it itself.
    #[serde(default)]
    pub play: String,
    /// Default voice when the config leaves `voice` empty.
    #[serde(default)]
    pub voice: String,
    /// Model path or name, for engines that need one.
    #[serde(default)]
    pub model: String,
    /// Anything else to splice in via `{extra}`.
    #[serde(default)]
    pub extra: String,
    /// Feed the sentence on stdin instead of as an argument. Piper and several
    /// other CLIs read their text that way.
    #[serde(default)]
    pub stdin: bool,
    /// One-line description shown by `/tts`.
    #[serde(default)]
    pub about: String,
}

/// Built-in engine presets.
///
/// Chosen from the local-TTS benchmark at <https://github.com/5uck1ess/tts-bench>
/// (June 2026 pass): small, permissively licensed, and fast enough to stay ahead
/// of a reader on CPU or Apple Silicon. See `docs/tts.md` for the numbers.
pub fn presets() -> BTreeMap<String, EngineSpec> {
    let mut out = BTreeMap::new();

    // Kokoro-82M — Apache-2.0, multilingual (including Chinese), 13.8× realtime
    // on M4 MPS. The default: best quality per megabyte for long-form reading.
    out.insert(
        "kokoro".to_string(),
        EngineSpec {
            synth: "kokoro-tts --text {text} --output {out} --voice {voice} --speed {rate}"
                .to_string(),
            play: default_player(),
            voice: "zf_xiaobei".to_string(),
            model: String::new(),
            extra: String::new(),
            stdin: false,
            about: "Kokoro-82M · Apache-2.0 · multilingual, best on long passages".to_string(),
        },
    );

    // Piper — ~15M, 62 ms warm TTFA and 33.5× realtime on M4: the one to pick
    // when you want speech to start before you notice you asked for it.
    out.insert(
        "piper".to_string(),
        EngineSpec {
            synth: "piper --model {model} --output_file {out} --length_scale {rate}".to_string(),
            play: default_player(),
            voice: String::new(),
            model: "~/.readio/voices/zh_CN-huayan-medium.onnx".to_string(),
            extra: String::new(),
            stdin: true,
            about: "Piper · GPL-3.0 · fastest to first sound, models are a few MB".to_string(),
        },
    );

    // Supertonic — 99M, pure ONNX with no torch anywhere, MIT, 31 languages.
    out.insert(
        "supertonic".to_string(),
        EngineSpec {
            synth: "supertonic --text {text} --out {out} --voice {voice}".to_string(),
            play: default_player(),
            voice: String::new(),
            model: String::new(),
            extra: String::new(),
            stdin: false,
            about: "Supertonic 99M · MIT · pure ONNX, no torch".to_string(),
        },
    );

    // Any OpenAI-compatible /v1/audio/speech server, which is how most local
    // model servers expose themselves (Kokoro-FastAPI and friends).
    out.insert(
        "openai".to_string(),
        EngineSpec {
            synth: "curl -sS --fail -X POST http://127.0.0.1:8880/v1/audio/speech \
                    -H 'Content-Type: application/json' \
                    -d {json} -o {out}"
                .to_string(),
            play: default_player(),
            voice: "zf_xiaobei".to_string(),
            model: "kokoro".to_string(),
            extra: String::new(),
            stdin: false,
            about: "Any OpenAI-compatible /v1/audio/speech endpoint".to_string(),
        },
    );

    out
}

/// Player command for this platform, chosen from what is normally present.
fn default_player() -> String {
    if cfg!(target_os = "macos") {
        "afplay {file}".to_string()
    } else if cfg!(target_os = "windows") {
        "powershell -c (New-Object Media.SoundPlayer {file}).PlaySync()".to_string()
    } else {
        // ALSA is the most common; ffplay is the usual fallback and is quiet
        // enough with these flags.
        "aplay -q {file}".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_wired_up() {
        for (name, spec) in presets() {
            assert!(
                spec.synth.contains("{out}"),
                "{name} must write to {{out}} so its duration can be measured"
            );
            assert!(
                spec.synth.contains("{text}") || spec.synth.contains("{json}") || spec.stdin,
                "{name} has no way to receive the text: neither a placeholder nor stdin"
            );
            assert!(!spec.about.is_empty(), "{name} has no description");
            assert!(
                spec.play.contains("{file}"),
                "{name} needs a player for its clip"
            );
        }
    }

    #[test]
    fn the_presets_cover_the_shapes_engines_come_in() {
        let presets = presets();
        assert!(
            presets.len() >= 4,
            "a text argument, stdin, and an HTTP server should all be represented"
        );
        assert!(
            presets["piper"].stdin,
            "piper reads its text on stdin, not as an argument"
        );
        assert!(
            presets["openai"].synth.contains("{json}"),
            "the OpenAI-compatible preset posts a JSON body"
        );
        assert!(
            !presets["kokoro"].voice.is_empty(),
            "the default engine should bring a working voice"
        );
    }

    #[test]
    fn the_player_is_platform_appropriate() {
        let player = default_player();
        assert!(
            player.contains("{file}"),
            "the player needs the clip: {player}"
        );
        if cfg!(target_os = "macos") {
            assert!(player.starts_with("afplay"), "got: {player}");
        }
    }
}
