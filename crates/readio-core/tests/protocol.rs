use readio_core::playback::{PlaybackEffect, PlaybackEvent};
use serde_json::json;

#[test]
fn playback_events_use_a_stable_cross_language_tagged_shape() {
    let event = PlaybackEvent::ClockTick {
        generation: 7,
        monotonic_ms: 7_000,
        unix_ms: 50_000,
        audio_position_ms: 640,
    };

    let value = serde_json::to_value(event).expect("serialize event");

    assert_eq!(
        value,
        json!({
            "type": "clock_tick",
            "generation": 7,
            "monotonic_ms": 7_000,
            "unix_ms": 50_000,
            "audio_position_ms": 640,
        })
    );
}

#[test]
fn playback_effects_use_the_same_tagged_protocol() {
    let effect = PlaybackEffect::PrefetchAudio {
        sentences: vec![2, 3],
    };

    let value = serde_json::to_value(effect).expect("serialize effect");

    assert_eq!(
        value,
        json!({
            "type": "prefetch_audio",
            "sentences": [2, 3],
        })
    );
}
