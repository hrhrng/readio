use readio_core::reading::{
    EFFORT_LEVELS, EffortLadder, EffortLevel, MAX_TEXT_DWELL_MS, MIN_TEXT_DWELL_MS, ReadingMode,
    ReadingPreferences, TextAdvanceEffect, TextAdvanceEvent, TextAdvanceState, TextAdvanceStatus,
    reduce_text_advance, resolve_reading_pace,
};

#[test]
fn reading_modes_are_three_persistent_peers_with_distinct_effect_ownership() {
    let modes = [
        ReadingMode::Manual,
        ReadingMode::Auto,
        ReadingMode::ReadAloud,
    ];

    assert_eq!(ReadingMode::default(), ReadingMode::Manual);
    assert_eq!(ReadingMode::Manual.next(), ReadingMode::Auto);
    assert_eq!(ReadingMode::Auto.next(), ReadingMode::ReadAloud);
    assert_eq!(ReadingMode::ReadAloud.next(), ReadingMode::Manual);
    assert_eq!(
        modes.map(ReadingMode::advances_automatically),
        [false, true, true]
    );
    assert_eq!(modes.map(ReadingMode::uses_audio), [false, false, true]);
    assert_eq!(
        modes.map(|mode| serde_json::to_string(&mode).expect("serialize mode")),
        ["\"manual\"", "\"auto\"", "\"read_aloud\""]
    );
}

#[test]
fn one_six_level_effort_multiplier_drives_both_text_and_audio_pace() {
    assert_eq!(
        EFFORT_LEVELS,
        [
            EffortLevel::Minimal,
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Xhigh,
            EffortLevel::Max,
        ]
    );
    assert_eq!(EffortLevel::default(), EffortLevel::High);

    let projection = resolve_reading_pace(&ReadingPreferences {
        mode: ReadingMode::ReadAloud,
        base_text_characters_per_second: 40.0,
        effort: EffortLevel::Low,
        multipliers: EffortLadder::default(),
    });

    assert_eq!(projection.mode, ReadingMode::ReadAloud);
    assert_eq!(projection.effort, EffortLevel::Low);
    assert_eq!(projection.multiplier, 2.0);
    assert_eq!(projection.text_characters_per_second, 80.0);
    assert_eq!(projection.audio_playback_rate, 2.0);
    assert!(projection.advances_automatically);
    assert!(projection.uses_audio);
}

#[test]
fn hand_edited_pace_values_normalize_to_the_shared_supported_range() {
    let too_fast = resolve_reading_pace(&ReadingPreferences {
        base_text_characters_per_second: 10_000.0,
        effort: EffortLevel::Minimal,
        multipliers: EffortLadder {
            minimal: 99.0,
            ..EffortLadder::default()
        },
        ..ReadingPreferences::default()
    });
    assert_eq!(too_fast.multiplier, 3.0);
    assert_eq!(too_fast.audio_playback_rate, 3.0);
    assert_eq!(too_fast.text_characters_per_second, 4_000.0);

    let not_numbers = resolve_reading_pace(&ReadingPreferences {
        base_text_characters_per_second: f32::NAN,
        effort: EffortLevel::Max,
        multipliers: EffortLadder {
            max: f32::INFINITY,
            ..EffortLadder::default()
        },
        ..ReadingPreferences::default()
    });
    assert_eq!(not_numbers.multiplier, 1.0);
    assert_eq!(not_numbers.text_characters_per_second, 46.0);
    assert_eq!(not_numbers.audio_playback_rate, 1.0);
}

#[test]
fn effort_cycle_visits_every_level_and_wraps() {
    let mut level = EffortLevel::Minimal;
    let mut seen = vec![level];
    for _ in 0..EFFORT_LEVELS.len() {
        level = level.next();
        seen.push(level);
    }

    assert_eq!(seen.first(), seen.last());
    assert_eq!(seen.len(), 7);
}

#[test]
fn auto_text_start_uses_rust_pace_and_a_monotonic_deadline() {
    let state = TextAdvanceState::new(3);
    let transition = reduce_text_advance(
        state,
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Running);
    assert_eq!(transition.state.current_sentence, 0);
    assert_eq!(transition.state.generation, 1);
    assert_eq!(transition.state.deadline_monotonic_ms, Some(12_000));
    assert_eq!(
        transition.effects,
        vec![TextAdvanceEffect::ScheduleAdvance {
            generation: 1,
            after_ms: 2_000,
        }]
    );
}

#[test]
fn pausing_auto_text_preserves_remaining_dwell_and_invalidates_its_generation() {
    let running = reduce_text_advance(
        TextAdvanceState::new(2),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;

    let transition = reduce_text_advance(
        running,
        TextAdvanceEvent::PauseRequested {
            monotonic_ms: 10_500,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Paused);
    assert_eq!(transition.state.generation, 2);
    assert_eq!(transition.state.deadline_monotonic_ms, None);
    assert_eq!(transition.state.remaining_ms, Some(1_500));
    assert_eq!(
        transition.effects,
        vec![TextAdvanceEffect::CancelScheduledAdvance]
    );
}

#[test]
fn resuming_auto_text_consumes_the_frozen_remaining_dwell() {
    let running = reduce_text_advance(
        TextAdvanceState::new(2),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;
    let paused = reduce_text_advance(
        running,
        TextAdvanceEvent::PauseRequested {
            monotonic_ms: 10_500,
        },
    )
    .state;

    let transition = reduce_text_advance(
        paused,
        TextAdvanceEvent::StartRequested {
            character_count: 400,
            text_characters_per_second: 20.0,
            monotonic_ms: 20_000,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Running);
    assert_eq!(transition.state.generation, 3);
    assert_eq!(transition.state.deadline_monotonic_ms, Some(21_500));
    assert_eq!(transition.state.remaining_ms, None);
    assert_eq!(
        transition.effects,
        vec![TextAdvanceEffect::ScheduleAdvance {
            generation: 3,
            after_ms: 1_500,
        }]
    );
}

#[test]
fn a_due_auto_text_timer_advances_and_schedules_from_the_next_sentence() {
    let running = reduce_text_advance(
        TextAdvanceState::new(3),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;

    let transition = reduce_text_advance(
        running,
        TextAdvanceEvent::AdvanceDue {
            generation: 1,
            monotonic_ms: 12_000,
            next_character_count: 40,
            text_characters_per_second: 40.0,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Running);
    assert_eq!(transition.state.current_sentence, 1);
    assert_eq!(transition.state.generation, 2);
    assert_eq!(transition.state.deadline_monotonic_ms, Some(13_000));
    assert_eq!(
        transition.effects,
        vec![
            TextAdvanceEffect::PersistProgress { sentence: 1 },
            TextAdvanceEffect::ScheduleAdvance {
                generation: 2,
                after_ms: 1_000,
            },
        ]
    );
}

#[test]
fn seeking_auto_text_invalidates_the_old_timer_and_reschedules_the_target() {
    let running = reduce_text_advance(
        TextAdvanceState::new(3),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;

    let transition = reduce_text_advance(
        running,
        TextAdvanceEvent::SeekRequested {
            sentence: 99,
            character_count: 120,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_500,
            autoplay: true,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Running);
    assert_eq!(transition.state.current_sentence, 2);
    assert_eq!(transition.state.generation, 2);
    assert_eq!(transition.state.deadline_monotonic_ms, Some(13_500));
    assert_eq!(
        transition.effects,
        vec![
            TextAdvanceEffect::CancelScheduledAdvance,
            TextAdvanceEffect::PersistProgress { sentence: 2 },
            TextAdvanceEffect::ScheduleAdvance {
                generation: 2,
                after_ms: 3_000,
            },
        ]
    );
}

#[test]
fn the_last_auto_text_sentence_persists_and_completes() {
    let running = reduce_text_advance(
        TextAdvanceState::new(1),
        TextAdvanceEvent::StartRequested {
            character_count: 40,
            text_characters_per_second: 40.0,
            monotonic_ms: 5_000,
        },
    )
    .state;

    let transition = reduce_text_advance(
        running,
        TextAdvanceEvent::AdvanceDue {
            generation: 1,
            monotonic_ms: 6_000,
            next_character_count: 0,
            text_characters_per_second: 40.0,
        },
    );

    assert_eq!(transition.state.status, TextAdvanceStatus::Completed);
    assert_eq!(transition.state.current_sentence, 0);
    assert_eq!(transition.state.deadline_monotonic_ms, None);
    assert_eq!(
        transition.effects,
        vec![
            TextAdvanceEffect::PersistProgress { sentence: 0 },
            TextAdvanceEffect::TextAdvanceCompleted,
        ]
    );
}

#[test]
fn a_text_timer_from_before_seek_cannot_advance_the_new_cursor() {
    let running = reduce_text_advance(
        TextAdvanceState::new(3),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;
    let sought = reduce_text_advance(
        running,
        TextAdvanceEvent::SeekRequested {
            sentence: 1,
            character_count: 40,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_500,
            autoplay: true,
        },
    )
    .state;

    let transition = reduce_text_advance(
        sought.clone(),
        TextAdvanceEvent::AdvanceDue {
            generation: 1,
            monotonic_ms: 99_000,
            next_character_count: 40,
            text_characters_per_second: 40.0,
        },
    );

    assert_eq!(transition.state, sought);
    assert!(transition.effects.is_empty());
}

#[test]
fn a_valid_text_timer_may_arrive_after_its_deadline_without_clock_underflow() {
    let running = reduce_text_advance(
        TextAdvanceState::new(2),
        TextAdvanceEvent::StartRequested {
            character_count: 80,
            text_characters_per_second: 40.0,
            monotonic_ms: 10_000,
        },
    )
    .state;

    let transition = reduce_text_advance(
        running,
        TextAdvanceEvent::AdvanceDue {
            generation: 1,
            monotonic_ms: 12_500,
            next_character_count: 40,
            text_characters_per_second: 40.0,
        },
    );

    assert_eq!(transition.state.current_sentence, 1);
    assert_eq!(transition.state.deadline_monotonic_ms, Some(13_500));
}

#[test]
fn auto_text_dwell_is_bounded_for_empty_and_pathological_sentences() {
    let empty = reduce_text_advance(
        TextAdvanceState::new(1),
        TextAdvanceEvent::StartRequested {
            character_count: 0,
            text_characters_per_second: 40.0,
            monotonic_ms: 1_000,
        },
    );
    assert_eq!(
        empty.effects,
        vec![TextAdvanceEffect::ScheduleAdvance {
            generation: 1,
            after_ms: MIN_TEXT_DWELL_MS,
        }]
    );

    let pathological = reduce_text_advance(
        TextAdvanceState::new(1),
        TextAdvanceEvent::StartRequested {
            character_count: 100_000_000,
            text_characters_per_second: 4.0,
            monotonic_ms: 1_000,
        },
    );
    assert_eq!(
        pathological.effects,
        vec![TextAdvanceEffect::ScheduleAdvance {
            generation: 1,
            after_ms: MAX_TEXT_DWELL_MS,
        }]
    );
}
