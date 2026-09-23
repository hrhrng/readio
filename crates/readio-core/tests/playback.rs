use readio_core::playback::{
    FetchPriority, PlaybackEffect, PlaybackEvent, PlaybackPolicy, PlaybackState, PlaybackStatus,
    reduce,
};
use serde_json::json;

#[test]
fn play_requests_the_current_sentence_and_enters_loading() {
    let state = PlaybackState::new(3);

    let transition = reduce(
        &PlaybackPolicy::default(),
        state,
        PlaybackEvent::PlayRequested,
    );

    assert_eq!(transition.state.status, PlaybackStatus::Loading);
    assert_eq!(transition.state.current_sentence, 0);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::FetchAudio {
                sentence: 0,
                generation: 1,
                priority: FetchPriority::User,
            },
            PlaybackEffect::PrefetchAudio {
                sentences: vec![1, 2],
            },
        ]
    );
}

#[test]
fn initial_play_prefetches_the_next_policy_window() {
    let policy = PlaybackPolicy {
        prefetch_window: 2,
        ..PlaybackPolicy::default()
    };

    let transition = reduce(&policy, PlaybackState::new(4), PlaybackEvent::PlayRequested);

    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::FetchAudio {
                sentence: 0,
                generation: 1,
                priority: FetchPriority::User,
            },
            PlaybackEffect::PrefetchAudio {
                sentences: vec![1, 2],
            },
        ]
    );
}

#[test]
fn ready_audio_starts_playback_and_schedules_the_platform_clock() {
    let policy = PlaybackPolicy {
        tick_interval_ms: 120,
        ..PlaybackPolicy::default()
    };
    let loading = reduce(&policy, PlaybackState::new(3), PlaybackEvent::PlayRequested).state;

    let transition = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 4_200,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Playing);
    assert_eq!(transition.state.current_duration_ms, Some(4_200));
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PlayAudio {
                sentence: 0,
                generation: 1,
                offset_ms: 0,
            },
            PlaybackEffect::ScheduleTick {
                generation: 1,
                after_ms: 120,
            },
        ]
    );
}

#[test]
fn stale_audio_completion_cannot_take_over_the_current_cursor() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(3), PlaybackEvent::PlayRequested).state;

    let transition = reduce(
        &policy,
        loading.clone(),
        PlaybackEvent::AudioReady {
            sentence: 2,
            generation: 1,
            duration_ms: 1_000,
        },
    );

    assert_eq!(transition.state, loading);
    assert!(transition.effects.is_empty());
}

#[test]
fn clock_sample_records_the_observed_audio_cursor_and_requests_the_next_tick() {
    let policy = PlaybackPolicy {
        tick_interval_ms: 80,
        ..PlaybackPolicy::default()
    };
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;

    let transition = reduce(
        &policy,
        playing,
        PlaybackEvent::ClockTick {
            generation: 1,
            monotonic_ms: 7_000,
            unix_ms: 50_000,
            audio_position_ms: 640,
        },
    );

    assert_eq!(transition.state.current_position_ms, 640);
    assert_eq!(transition.state.last_tick_monotonic_ms, Some(7_000));
    assert_eq!(
        transition.effects,
        vec![PlaybackEffect::ScheduleTick {
            generation: 1,
            after_ms: 80,
        }]
    );
}

#[test]
fn wall_clock_sleep_deadline_pauses_and_persists_without_rescheduling() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(1), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 5_000,
        },
    )
    .state;
    let armed = reduce(
        &policy,
        playing,
        PlaybackEvent::SleepTimerSet {
            deadline_unix_ms: 20_000,
        },
    )
    .state;

    let transition = reduce(
        &policy,
        armed,
        PlaybackEvent::ClockTick {
            generation: 1,
            monotonic_ms: 3_000,
            unix_ms: 20_000,
            audio_position_ms: 1_250,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Paused);
    assert_eq!(transition.state.current_position_ms, 1_250);
    assert_eq!(transition.state.sleep_deadline_unix_ms, None);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PauseAudio,
            PlaybackEffect::PersistProgress {
                sentence: 0,
                position_ms: 1_250,
            },
        ]
    );
}

#[test]
fn clearing_a_sleep_deadline_prevents_the_old_deadline_from_pausing_playback() {
    let policy = PlaybackPolicy {
        tick_interval_ms: 80,
        ..PlaybackPolicy::default()
    };
    let loading = reduce(&policy, PlaybackState::new(1), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 5_000,
        },
    )
    .state;
    let armed = reduce(
        &policy,
        playing,
        PlaybackEvent::SleepTimerSet {
            deadline_unix_ms: 20_000,
        },
    )
    .state;

    let cleared = reduce(&policy, armed, PlaybackEvent::SleepTimerCleared);
    assert_eq!(cleared.state.sleep_deadline_unix_ms, None);
    assert!(cleared.effects.is_empty());

    let after_old_deadline = reduce(
        &policy,
        cleared.state,
        PlaybackEvent::ClockTick {
            generation: 1,
            monotonic_ms: 3_000,
            unix_ms: 25_000,
            audio_position_ms: 1_250,
        },
    );
    assert_eq!(after_old_deadline.state.status, PlaybackStatus::Playing);
    assert_eq!(
        after_old_deadline.effects,
        vec![PlaybackEffect::ScheduleTick {
            generation: 1,
            after_ms: 80,
        }]
    );
}

#[test]
fn ending_a_sentence_advances_the_cursor_and_prefetches_from_policy() {
    let policy = PlaybackPolicy {
        prefetch_window: 2,
        ..PlaybackPolicy::default()
    };
    let loading = reduce(&policy, PlaybackState::new(4), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;

    let transition = reduce(
        &policy,
        playing,
        PlaybackEvent::AudioEnded {
            sentence: 0,
            generation: 1,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Loading);
    assert_eq!(transition.state.current_sentence, 1);
    assert_eq!(transition.state.current_duration_ms, None);
    assert_eq!(transition.state.current_position_ms, 0);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PersistProgress {
                sentence: 1,
                position_ms: 0,
            },
            PlaybackEffect::FetchAudio {
                sentence: 1,
                generation: 2,
                priority: FetchPriority::User,
            },
            PlaybackEffect::PrefetchAudio {
                sentences: vec![2, 3],
            },
        ]
    );
}

#[test]
fn ending_the_last_sentence_completes_and_persists_the_final_cursor() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(1), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 900,
        },
    )
    .state;

    let transition = reduce(
        &policy,
        playing,
        PlaybackEvent::AudioEnded {
            sentence: 0,
            generation: 1,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Completed);
    assert_eq!(transition.state.current_position_ms, 900);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PersistProgress {
                sentence: 0,
                position_ms: 900,
            },
            PlaybackEffect::PlaybackCompleted,
        ]
    );
}

#[test]
fn play_on_an_empty_document_completes_without_fetching() {
    let transition = reduce(
        &PlaybackPolicy::default(),
        PlaybackState::new(0),
        PlaybackEvent::PlayRequested,
    );

    assert_eq!(transition.state.status, PlaybackStatus::Completed);
    assert!(transition.effects.is_empty());
}

#[test]
fn pause_samples_the_real_audio_cursor_and_persists_it_once() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;

    let transition = reduce(
        &policy,
        playing,
        PlaybackEvent::PauseRequested {
            audio_position_ms: 701,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Paused);
    assert_eq!(transition.state.current_position_ms, 701);
    assert_eq!(transition.state.last_tick_monotonic_ms, None);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PauseAudio,
            PlaybackEffect::PersistProgress {
                sentence: 0,
                position_ms: 701,
            },
        ]
    );
}

#[test]
fn play_after_pause_resumes_the_same_audio_clip_at_the_sampled_cursor() {
    let policy = PlaybackPolicy {
        tick_interval_ms: 90,
        ..PlaybackPolicy::default()
    };
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;
    let paused = reduce(
        &policy,
        playing,
        PlaybackEvent::PauseRequested {
            audio_position_ms: 701,
        },
    )
    .state;

    let transition = reduce(&policy, paused, PlaybackEvent::PlayRequested);

    assert_eq!(transition.state.status, PlaybackStatus::Playing);
    assert_eq!(transition.state.current_position_ms, 701);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PlayAudio {
                sentence: 0,
                generation: 3,
                offset_ms: 701,
            },
            PlaybackEffect::ScheduleTick {
                generation: 3,
                after_ms: 90,
            },
        ]
    );
}

#[test]
fn repeated_play_and_pause_requests_are_idempotent() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(1), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 1_000,
        },
    )
    .state;

    let duplicate_play = reduce(&policy, playing.clone(), PlaybackEvent::PlayRequested);
    assert_eq!(duplicate_play.state, playing);
    assert!(duplicate_play.effects.is_empty());

    let paused = reduce(
        &policy,
        playing,
        PlaybackEvent::PauseRequested {
            audio_position_ms: 250,
        },
    )
    .state;
    let duplicate_pause = reduce(
        &policy,
        paused.clone(),
        PlaybackEvent::PauseRequested {
            audio_position_ms: 999,
        },
    );
    assert_eq!(duplicate_pause.state, paused);
    assert!(duplicate_pause.effects.is_empty());
}

#[test]
fn seek_while_loading_targets_the_exact_cursor_with_a_new_generation() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(4), PlaybackEvent::PlayRequested).state;
    let seek = serde_json::from_value(json!({
        "type": "seek_requested",
        "sentence": 2,
        "position_ms": 450,
        "autoplay": true,
    }))
    .expect("seek event uses the portable playback protocol");

    let transition = reduce(&policy, loading, seek);
    let state = serde_json::to_value(&transition.state).expect("serialize state");
    let effects = serde_json::to_value(&transition.effects).expect("serialize effects");

    assert_eq!(state["status"], json!("loading"));
    assert_eq!(state["current_sentence"], json!(2));
    assert_eq!(state["current_position_ms"], json!(450));
    assert_eq!(state["generation"], json!(2));
    assert_eq!(
        effects,
        json!([
            { "type": "pause_audio" },
            {
                "type": "persist_progress",
                "sentence": 2,
                "position_ms": 450,
            },
            {
                "type": "fetch_audio",
                "sentence": 2,
                "generation": 2,
                "priority": "user",
            },
        ])
    );
}

#[test]
fn an_old_audio_result_cannot_take_over_after_seeking_away_and_back() {
    let policy = PlaybackPolicy::default();
    let first_load = reduce(&policy, PlaybackState::new(3), PlaybackEvent::PlayRequested).state;
    let seek_away = serde_json::from_value(json!({
        "type": "seek_requested",
        "sentence": 1,
        "position_ms": 0,
        "autoplay": true,
    }))
    .expect("seek away");
    let away = reduce(&policy, first_load, seek_away).state;
    let seek_back = serde_json::from_value(json!({
        "type": "seek_requested",
        "sentence": 0,
        "position_ms": 250,
        "autoplay": true,
    }))
    .expect("seek back");
    let current = reduce(&policy, away, seek_back).state;
    let stale_audio = serde_json::from_value(json!({
        "type": "audio_ready",
        "sentence": 0,
        "generation": 1,
        "duration_ms": 2_000,
    }))
    .expect("audio result");

    let transition = reduce(&policy, current.clone(), stale_audio);

    assert_eq!(current.generation, 3);
    assert_eq!(transition.state, current);
    assert!(transition.effects.is_empty());
}

#[test]
fn requested_position_survives_loading_and_starts_at_the_exact_offset() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(3), PlaybackEvent::PlayRequested).state;
    let seek = PlaybackEvent::SeekRequested {
        sentence: 2,
        position_ms: 450,
        autoplay: true,
    };
    let seeking = reduce(&policy, loading, seek).state;

    let transition = reduce(
        &policy,
        seeking,
        PlaybackEvent::AudioReady {
            sentence: 2,
            generation: 2,
            duration_ms: 2_000,
        },
    );

    assert_eq!(transition.state.current_position_ms, 450);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PlayAudio {
                sentence: 2,
                generation: 2,
                offset_ms: 450,
            },
            PlaybackEffect::ScheduleTick {
                generation: 2,
                after_ms: 250,
            },
        ]
    );
}

#[test]
fn an_ended_callback_from_before_pause_cannot_advance_resumed_playback() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;
    let paused = reduce(
        &policy,
        playing,
        PlaybackEvent::PauseRequested {
            audio_position_ms: 600,
        },
    )
    .state;
    let resumed = reduce(&policy, paused, PlaybackEvent::PlayRequested).state;
    let stale_ended = serde_json::from_value(json!({
        "type": "audio_ended",
        "sentence": 0,
        "generation": 1,
    }))
    .expect("ended event");

    let transition = reduce(&policy, resumed.clone(), stale_ended);

    assert_eq!(resumed.generation, 3);
    assert_eq!(transition.state, resumed);
    assert!(transition.effects.is_empty());
}

#[test]
fn a_clock_tick_from_before_seek_cannot_overwrite_the_new_audio_cursor() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;
    let current = reduce(
        &policy,
        playing,
        PlaybackEvent::SeekRequested {
            sentence: 0,
            position_ms: 500,
            autoplay: true,
        },
    )
    .state;
    let stale_tick = serde_json::from_value(json!({
        "type": "clock_tick",
        "generation": 1,
        "monotonic_ms": 9_000,
        "unix_ms": 50_000,
        "audio_position_ms": 1_700,
    }))
    .expect("clock event");

    let transition = reduce(&policy, current.clone(), stale_tick);

    assert_eq!(current.generation, 2);
    assert_eq!(transition.state, current);
    assert!(transition.effects.is_empty());
}

#[test]
fn pause_during_loading_cancels_the_pending_generation() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;

    let transition = reduce(
        &policy,
        loading,
        PlaybackEvent::PauseRequested {
            audio_position_ms: 0,
        },
    );

    assert_eq!(transition.state.status, PlaybackStatus::Paused);
    assert_eq!(transition.state.generation, 2);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PauseAudio,
            PlaybackEffect::PersistProgress {
                sentence: 0,
                position_ms: 0,
            },
        ]
    );
}

#[test]
fn current_audio_failure_enters_a_portable_failed_state() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let failed = serde_json::from_value(json!({
        "type": "audio_failed",
        "sentence": 0,
        "generation": 1,
        "message": "TTS synthesis failed",
    }))
    .expect("failure event uses the portable playback protocol");

    let transition = reduce(&policy, loading, failed);
    let state = serde_json::to_value(&transition.state).expect("serialize state");
    let effects = serde_json::to_value(&transition.effects).expect("serialize effects");

    assert_eq!(state["status"], json!("failed"));
    assert_eq!(state["generation"], json!(2));
    assert_eq!(state["error"], json!("TTS synthesis failed"));
    assert_eq!(
        effects,
        json!([
            { "type": "pause_audio" },
            {
                "type": "playback_failed",
                "sentence": 0,
                "message": "TTS synthesis failed",
            },
        ])
    );
}

#[test]
fn changing_the_audio_source_reloads_the_active_sentence_without_losing_position() {
    let policy = PlaybackPolicy::default();
    let loading = reduce(&policy, PlaybackState::new(2), PlaybackEvent::PlayRequested).state;
    let playing = reduce(
        &policy,
        loading,
        PlaybackEvent::AudioReady {
            sentence: 0,
            generation: 1,
            duration_ms: 2_000,
        },
    )
    .state;
    let sampled = reduce(
        &policy,
        playing,
        PlaybackEvent::ClockTick {
            generation: 1,
            monotonic_ms: 10,
            unix_ms: 20,
            audio_position_ms: 600,
        },
    )
    .state;
    let source_changed = serde_json::from_value(json!({
        "type": "audio_source_changed",
    }))
    .expect("audio-source event uses the portable playback protocol");

    let transition = reduce(&policy, sampled, source_changed);

    assert_eq!(transition.state.status, PlaybackStatus::Loading);
    assert_eq!(transition.state.generation, 2);
    assert_eq!(transition.state.current_position_ms, 600);
    assert_eq!(transition.state.current_duration_ms, None);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::PauseAudio,
            PlaybackEffect::FetchAudio {
                sentence: 0,
                generation: 2,
                priority: FetchPriority::User,
            },
        ]
    );
}
