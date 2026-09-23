use readio_core::reading::{
    AudioDurationObservation, ReadingProgressRequest, ReadingTimingEvidence, RemainingTimeKind,
    project_reading_progress, resolve_reading_seek,
};

#[test]
fn scalar_progress_uses_character_position_instead_of_sentence_count() {
    let projection = project_reading_progress(ReadingProgressRequest {
        segment_character_counts: vec![2, 8],
        current_segment: 1,
        current_segment_progress_basis_points: 5_000,
        timing: ReadingTimingEvidence::Unknown,
    });

    assert_eq!(projection.char_count, 11);
    assert_eq!(projection.char_offset, 7);
    assert_eq!(projection.progress_basis_points, 6_363);
    assert_eq!(projection.remaining_characters, 4);
}

#[test]
fn unknown_timing_evidence_never_fabricates_remaining_time() {
    let projection = project_reading_progress(ReadingProgressRequest {
        segment_character_counts: vec![120, 80],
        current_segment: 0,
        current_segment_progress_basis_points: 0,
        timing: ReadingTimingEvidence::Unknown,
    });

    assert_eq!(projection.remaining_time.kind, RemainingTimeKind::Unknown);
    assert_eq!(projection.remaining_time.seconds, None);
    assert_eq!(projection.remaining_time.sample_count, 0);
}

#[test]
fn text_rate_evidence_projects_remaining_scalar_characters() {
    let projection = project_reading_progress(ReadingProgressRequest {
        segment_character_counts: vec![4, 4],
        current_segment: 1,
        current_segment_progress_basis_points: 0,
        timing: ReadingTimingEvidence::TextRate {
            characters_per_second: 4.0,
        },
    });

    assert_eq!(projection.char_offset, 5);
    assert_eq!(projection.remaining_characters, 4);
    assert_eq!(projection.remaining_time.kind, RemainingTimeKind::TextRate);
    assert_eq!(projection.remaining_time.seconds, Some(1));
}

#[test]
fn audio_evidence_uses_real_durations_and_playback_rate() {
    let projection = project_reading_progress(ReadingProgressRequest {
        segment_character_counts: vec![100, 200, 100],
        current_segment: 1,
        current_segment_progress_basis_points: 0,
        timing: ReadingTimingEvidence::ObservedAudio {
            playback_rate: 2.0,
            current_position_ms: 500,
            durations: vec![
                AudioDurationObservation {
                    segment: 0,
                    duration_ms: 1_000,
                },
                AudioDurationObservation {
                    segment: 1,
                    duration_ms: 2_000,
                },
            ],
        },
    });

    assert_eq!(projection.char_offset, 151);
    assert_eq!(projection.progress_basis_points, 3_756);
    assert_eq!(
        projection.remaining_time.kind,
        RemainingTimeKind::ObservedAudio
    );
    assert_eq!(projection.remaining_time.seconds, Some(2));
    assert_eq!(projection.remaining_time.sample_count, 2);
}

#[test]
fn unusable_audio_observations_keep_remaining_time_unknown() {
    let projection = project_reading_progress(ReadingProgressRequest {
        segment_character_counts: vec![100],
        current_segment: 0,
        current_segment_progress_basis_points: 0,
        timing: ReadingTimingEvidence::ObservedAudio {
            playback_rate: 1.0,
            current_position_ms: 40,
            durations: vec![AudioDurationObservation {
                segment: 9,
                duration_ms: 1_000,
            }],
        },
    });

    assert_eq!(projection.char_offset, 0);
    assert_eq!(projection.remaining_time.kind, RemainingTimeKind::Unknown);
    assert_eq!(projection.remaining_time.seconds, None);
}

#[test]
fn progress_seek_resolves_against_scalar_length_not_segment_count() {
    let cursor = resolve_reading_seek(&[1, 9], 5_000);

    assert_eq!(cursor.segment, 1);
    assert_eq!(cursor.segment_progress_basis_points, 3_333);
    assert_eq!(cursor.char_offset, 5);
}
