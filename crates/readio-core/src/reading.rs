use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MIN_TEXT_CHARACTERS_PER_SECOND: f32 = 4.0;
pub const MAX_TEXT_CHARACTERS_PER_SECOND: f32 = 4_000.0;
pub const MIN_PLAYBACK_RATE: f32 = 0.5;
pub const MAX_PLAYBACK_RATE: f32 = 3.0;
pub const MIN_TEXT_DWELL_MS: u64 = 250;
pub const MAX_TEXT_DWELL_MS: u64 = 120_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingMode {
    #[default]
    Manual,
    Auto,
    ReadAloud,
}

impl ReadingMode {
    pub fn next(self) -> Self {
        match self {
            Self::Manual => Self::Auto,
            Self::Auto => Self::ReadAloud,
            Self::ReadAloud => Self::Manual,
        }
    }

    pub fn advances_automatically(self) -> bool {
        matches!(self, Self::Auto | Self::ReadAloud)
    }

    pub fn uses_audio(self) -> bool {
        self == Self::ReadAloud
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    Minimal,
    Low,
    Medium,
    #[default]
    High,
    Xhigh,
    Max,
}

pub const EFFORT_LEVELS: [EffortLevel; 6] = [
    EffortLevel::Minimal,
    EffortLevel::Low,
    EffortLevel::Medium,
    EffortLevel::High,
    EffortLevel::Xhigh,
    EffortLevel::Max,
];

impl EffortLevel {
    pub fn next(self) -> Self {
        let index = EFFORT_LEVELS
            .iter()
            .position(|level| *level == self)
            .unwrap_or(3);
        EFFORT_LEVELS[(index + 1) % EFFORT_LEVELS.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffortLadder {
    pub minimal: f32,
    pub low: f32,
    pub medium: f32,
    pub high: f32,
    pub xhigh: f32,
    pub max: f32,
}

impl Default for EffortLadder {
    fn default() -> Self {
        Self {
            minimal: 2.5,
            low: 2.0,
            medium: 1.5,
            high: 1.0,
            xhigh: 0.85,
            max: 0.7,
        }
    }
}

impl EffortLadder {
    pub fn multiplier(self, level: EffortLevel) -> f32 {
        let value = match level {
            EffortLevel::Minimal => self.minimal,
            EffortLevel::Low => self.low,
            EffortLevel::Medium => self.medium,
            EffortLevel::High => self.high,
            EffortLevel::Xhigh => self.xhigh,
            EffortLevel::Max => self.max,
        };
        normalize_multiplier(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingPreferences {
    pub mode: ReadingMode,
    pub base_text_characters_per_second: f32,
    pub effort: EffortLevel,
    pub multipliers: EffortLadder,
}

impl Default for ReadingPreferences {
    fn default() -> Self {
        Self {
            mode: ReadingMode::Manual,
            base_text_characters_per_second: 46.0,
            effort: EffortLevel::High,
            multipliers: EffortLadder::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReadingPace {
    pub mode: ReadingMode,
    pub effort: EffortLevel,
    pub multiplier: f32,
    pub text_characters_per_second: f32,
    pub audio_playback_rate: f32,
    pub advances_automatically: bool,
    pub uses_audio: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDurationObservation {
    pub segment: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReadingTimingEvidence {
    Unknown,
    TextRate {
        characters_per_second: f32,
    },
    ObservedAudio {
        playback_rate: f32,
        current_position_ms: u64,
        durations: Vec<AudioDurationObservation>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingProgressRequest {
    pub segment_character_counts: Vec<u64>,
    pub current_segment: usize,
    pub current_segment_progress_basis_points: u16,
    pub timing: ReadingTimingEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemainingTimeKind {
    Unknown,
    TextRate,
    ObservedAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemainingTimeProjection {
    pub kind: RemainingTimeKind,
    pub seconds: Option<u64>,
    pub sample_count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingProgressProjection {
    pub char_offset: u64,
    pub char_count: u64,
    pub progress_basis_points: u16,
    pub remaining_characters: u64,
    pub remaining_time: RemainingTimeProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingSeekCursor {
    pub segment: usize,
    pub segment_progress_basis_points: u16,
    pub char_offset: u64,
}

pub fn project_reading_progress(request: ReadingProgressRequest) -> ReadingProgressProjection {
    let char_count = canonical_character_count(&request.segment_character_counts);
    if request.segment_character_counts.is_empty() {
        return ReadingProgressProjection {
            char_offset: 0,
            char_count: 0,
            progress_basis_points: 0,
            remaining_characters: 0,
            remaining_time: remaining_time(
                &request.segment_character_counts,
                0,
                0,
                0,
                &request.timing,
            ),
        };
    }

    let current_segment = request
        .current_segment
        .min(request.segment_character_counts.len() - 1);
    let segment_progress_basis_points = match &request.timing {
        ReadingTimingEvidence::ObservedAudio {
            current_position_ms,
            durations,
            ..
        } => durations
            .iter()
            .rev()
            .find(|observation| {
                observation.segment == current_segment && observation.duration_ms > 0
            })
            .map(|observation| {
                basis_points(
                    (*current_position_ms).min(observation.duration_ms),
                    observation.duration_ms,
                )
            })
            .unwrap_or(request.current_segment_progress_basis_points.min(10_000)),
        _ => request.current_segment_progress_basis_points.min(10_000),
    };
    let char_offset = segment_start(&request.segment_character_counts, current_segment)
        .saturating_add(scaled_value(
            request.segment_character_counts[current_segment],
            segment_progress_basis_points,
        ))
        .min(char_count);

    ReadingProgressProjection {
        char_offset,
        char_count,
        progress_basis_points: basis_points(char_offset, char_count),
        remaining_characters: char_count.saturating_sub(char_offset),
        remaining_time: remaining_time(
            &request.segment_character_counts,
            current_segment,
            segment_progress_basis_points,
            char_offset,
            &request.timing,
        ),
    }
}

pub fn resolve_reading_seek(
    segment_character_counts: &[u64],
    progress_basis_points: u16,
) -> ReadingSeekCursor {
    if segment_character_counts.is_empty() {
        return ReadingSeekCursor {
            segment: 0,
            segment_progress_basis_points: 0,
            char_offset: 0,
        };
    }
    let char_count = canonical_character_count(segment_character_counts);
    let char_offset = scaled_value(char_count, progress_basis_points.min(10_000));
    let last = segment_character_counts.len() - 1;
    for (segment, length) in segment_character_counts.iter().copied().enumerate() {
        let start = segment_start(segment_character_counts, segment);
        let end = start.saturating_add(length);
        if char_offset < end || segment == last {
            return ReadingSeekCursor {
                segment,
                segment_progress_basis_points: basis_points(
                    char_offset.saturating_sub(start).min(length),
                    length,
                ),
                char_offset,
            };
        }
    }
    unreachable!("a non-empty segment list always resolves its final cursor")
}

fn remaining_time(
    counts: &[u64],
    current_segment: usize,
    segment_progress_basis_points: u16,
    char_offset: u64,
    evidence: &ReadingTimingEvidence,
) -> RemainingTimeProjection {
    match evidence {
        ReadingTimingEvidence::Unknown => unknown_remaining_time(),
        ReadingTimingEvidence::TextRate {
            characters_per_second,
        } if characters_per_second.is_finite() && *characters_per_second > 0.0 => {
            let remaining = canonical_character_count(counts).saturating_sub(char_offset);
            RemainingTimeProjection {
                kind: RemainingTimeKind::TextRate,
                seconds: Some((remaining as f64 / f64::from(*characters_per_second)).ceil() as u64),
                sample_count: 0,
            }
        }
        ReadingTimingEvidence::ObservedAudio {
            playback_rate,
            current_position_ms,
            durations,
        } if playback_rate.is_finite() && *playback_rate > 0.0 => {
            let observations = durations
                .iter()
                .filter(|observation| {
                    observation.duration_ms > 0
                        && counts.get(observation.segment).copied().unwrap_or(0) > 0
                })
                .fold(BTreeMap::new(), |mut result, observation| {
                    result.insert(observation.segment, observation.duration_ms);
                    result
                });
            if observations.is_empty() || counts.is_empty() {
                return unknown_remaining_time();
            }
            let observed_characters = observations.keys().fold(0_u128, |total, segment| {
                total.saturating_add(u128::from(counts[*segment]))
            });
            let observed_milliseconds = observations.values().fold(0_u128, |total, duration| {
                total.saturating_add(u128::from(*duration))
            });
            if observed_characters == 0 || observed_milliseconds == 0 {
                return unknown_remaining_time();
            }

            let current_length = counts[current_segment];
            let current_remaining_characters = current_length
                .saturating_sub(scaled_value(current_length, segment_progress_basis_points));
            let current_milliseconds = observations.get(&current_segment).map_or_else(
                || {
                    estimate_milliseconds(
                        current_remaining_characters,
                        observed_milliseconds,
                        observed_characters,
                    )
                },
                |duration| {
                    u128::from(duration.saturating_sub((*current_position_ms).min(*duration)))
                },
            );
            let future_milliseconds = counts
                .iter()
                .copied()
                .enumerate()
                .skip(current_segment + 1)
                .fold(0_u128, |total, (segment, characters)| {
                    total.saturating_add(observations.get(&segment).map_or_else(
                        || {
                            estimate_milliseconds(
                                characters,
                                observed_milliseconds,
                                observed_characters,
                            )
                        },
                        |duration| u128::from(*duration),
                    ))
                });
            let wall_milliseconds = (current_milliseconds.saturating_add(future_milliseconds)
                as f64
                / f64::from(*playback_rate))
            .ceil() as u64;
            RemainingTimeProjection {
                kind: RemainingTimeKind::ObservedAudio,
                seconds: Some(wall_milliseconds.saturating_add(999) / 1_000),
                sample_count: observations.len().min(usize::from(u16::MAX)) as u16,
            }
        }
        _ => unknown_remaining_time(),
    }
}

fn unknown_remaining_time() -> RemainingTimeProjection {
    RemainingTimeProjection {
        kind: RemainingTimeKind::Unknown,
        seconds: None,
        sample_count: 0,
    }
}

fn canonical_character_count(counts: &[u64]) -> u64 {
    counts
        .iter()
        .copied()
        .fold(0_u64, u64::saturating_add)
        .saturating_add(u64::try_from(counts.len().saturating_sub(1)).unwrap_or(u64::MAX))
}

fn segment_start(counts: &[u64], segment: usize) -> u64 {
    counts
        .iter()
        .take(segment)
        .copied()
        .fold(0_u64, u64::saturating_add)
        .saturating_add(u64::try_from(segment).unwrap_or(u64::MAX))
}

fn basis_points(value: u64, total: u64) -> u16 {
    if total == 0 {
        return 0;
    }
    ((u128::from(value) * 10_000) / u128::from(total)).min(10_000) as u16
}

fn scaled_value(value: u64, basis_points: u16) -> u64 {
    ((u128::from(value) * u128::from(basis_points.min(10_000))) / 10_000).min(u128::from(u64::MAX))
        as u64
}

fn estimate_milliseconds(characters: u64, total_ms: u128, total_characters: u128) -> u128 {
    (u128::from(characters) * total_ms).div_ceil(total_characters)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAdvanceStatus {
    #[default]
    Idle,
    Running,
    Paused,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAdvanceState {
    pub status: TextAdvanceStatus,
    pub current_sentence: usize,
    pub sentence_count: usize,
    pub generation: u64,
    pub deadline_monotonic_ms: Option<u64>,
    pub remaining_ms: Option<u64>,
}

impl TextAdvanceState {
    pub fn new(sentence_count: usize) -> Self {
        Self {
            status: TextAdvanceStatus::Idle,
            current_sentence: 0,
            sentence_count,
            generation: 0,
            deadline_monotonic_ms: None,
            remaining_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextAdvanceEvent {
    StartRequested {
        character_count: u64,
        text_characters_per_second: f32,
        monotonic_ms: u64,
    },
    PauseRequested {
        monotonic_ms: u64,
    },
    SeekRequested {
        sentence: usize,
        character_count: u64,
        text_characters_per_second: f32,
        monotonic_ms: u64,
        autoplay: bool,
    },
    AdvanceDue {
        generation: u64,
        monotonic_ms: u64,
        next_character_count: u64,
        text_characters_per_second: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextAdvanceEffect {
    ScheduleAdvance { generation: u64, after_ms: u64 },
    CancelScheduledAdvance,
    PersistProgress { sentence: usize },
    TextAdvanceCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAdvanceTransition {
    pub state: TextAdvanceState,
    pub effects: Vec<TextAdvanceEffect>,
}

pub fn reduce_text_advance(
    mut state: TextAdvanceState,
    event: TextAdvanceEvent,
) -> TextAdvanceTransition {
    match event {
        TextAdvanceEvent::StartRequested {
            character_count,
            text_characters_per_second,
            monotonic_ms,
        } => {
            if state.status == TextAdvanceStatus::Running {
                return TextAdvanceTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            if state.sentence_count == 0 {
                state.status = TextAdvanceStatus::Completed;
                return TextAdvanceTransition {
                    state,
                    effects: Vec::new(),
                };
            }

            let after_ms = state.remaining_ms.take().unwrap_or_else(|| {
                text_advance_duration_ms(character_count, text_characters_per_second)
            });
            state.generation = state.generation.saturating_add(1);
            state.status = TextAdvanceStatus::Running;
            state.deadline_monotonic_ms = Some(monotonic_ms.saturating_add(after_ms));
            TextAdvanceTransition {
                state: state.clone(),
                effects: vec![TextAdvanceEffect::ScheduleAdvance {
                    generation: state.generation,
                    after_ms,
                }],
            }
        }
        TextAdvanceEvent::PauseRequested { monotonic_ms } => {
            if state.status != TextAdvanceStatus::Running {
                return TextAdvanceTransition {
                    state,
                    effects: Vec::new(),
                };
            }

            state.generation = state.generation.saturating_add(1);
            state.status = TextAdvanceStatus::Paused;
            state.remaining_ms = state
                .deadline_monotonic_ms
                .map(|deadline| deadline.saturating_sub(monotonic_ms));
            state.deadline_monotonic_ms = None;
            TextAdvanceTransition {
                state,
                effects: vec![TextAdvanceEffect::CancelScheduledAdvance],
            }
        }
        TextAdvanceEvent::SeekRequested {
            sentence,
            character_count,
            text_characters_per_second,
            monotonic_ms,
            autoplay,
        } => {
            state.generation = state.generation.saturating_add(1);
            state.remaining_ms = None;
            state.deadline_monotonic_ms = None;
            if state.sentence_count == 0 {
                state.status = TextAdvanceStatus::Completed;
                return TextAdvanceTransition {
                    state,
                    effects: vec![TextAdvanceEffect::CancelScheduledAdvance],
                };
            }

            state.current_sentence = sentence.min(state.sentence_count - 1);
            let mut effects = vec![
                TextAdvanceEffect::CancelScheduledAdvance,
                TextAdvanceEffect::PersistProgress {
                    sentence: state.current_sentence,
                },
            ];
            if autoplay {
                let after_ms =
                    text_advance_duration_ms(character_count, text_characters_per_second);
                state.status = TextAdvanceStatus::Running;
                state.deadline_monotonic_ms = Some(monotonic_ms.saturating_add(after_ms));
                effects.push(TextAdvanceEffect::ScheduleAdvance {
                    generation: state.generation,
                    after_ms,
                });
            } else {
                state.status = TextAdvanceStatus::Paused;
            }
            TextAdvanceTransition { state, effects }
        }
        TextAdvanceEvent::AdvanceDue {
            generation,
            monotonic_ms,
            next_character_count,
            text_characters_per_second,
        } => {
            if state.status != TextAdvanceStatus::Running || generation != state.generation {
                return TextAdvanceTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            if let Some(remaining) = state
                .deadline_monotonic_ms
                .and_then(|deadline| (monotonic_ms < deadline).then(|| deadline - monotonic_ms))
            {
                return TextAdvanceTransition {
                    state: state.clone(),
                    effects: vec![TextAdvanceEffect::ScheduleAdvance {
                        generation: state.generation,
                        after_ms: remaining,
                    }],
                };
            }
            if state.current_sentence + 1 >= state.sentence_count {
                state.status = TextAdvanceStatus::Completed;
                state.deadline_monotonic_ms = None;
                return TextAdvanceTransition {
                    state: state.clone(),
                    effects: vec![
                        TextAdvanceEffect::PersistProgress {
                            sentence: state.current_sentence,
                        },
                        TextAdvanceEffect::TextAdvanceCompleted,
                    ],
                };
            }

            state.current_sentence += 1;
            state.generation = state.generation.saturating_add(1);
            let after_ms =
                text_advance_duration_ms(next_character_count, text_characters_per_second);
            state.deadline_monotonic_ms = Some(monotonic_ms.saturating_add(after_ms));
            TextAdvanceTransition {
                state: state.clone(),
                effects: vec![
                    TextAdvanceEffect::PersistProgress {
                        sentence: state.current_sentence,
                    },
                    TextAdvanceEffect::ScheduleAdvance {
                        generation: state.generation,
                        after_ms,
                    },
                ],
            }
        }
    }
}

fn text_advance_duration_ms(character_count: u64, characters_per_second: f32) -> u64 {
    let rate = normalize_text_rate(characters_per_second);
    ((character_count as f64 / f64::from(rate)) * 1_000.0)
        .round()
        .clamp(MIN_TEXT_DWELL_MS as f64, MAX_TEXT_DWELL_MS as f64) as u64
}

pub fn resolve_reading_pace(preferences: &ReadingPreferences) -> ReadingPace {
    let base = normalize_text_rate(preferences.base_text_characters_per_second);
    let multiplier = preferences.multipliers.multiplier(preferences.effort);
    ReadingPace {
        mode: preferences.mode,
        effort: preferences.effort,
        multiplier,
        text_characters_per_second: (base * multiplier).clamp(
            MIN_TEXT_CHARACTERS_PER_SECOND,
            MAX_TEXT_CHARACTERS_PER_SECOND,
        ),
        audio_playback_rate: multiplier,
        advances_automatically: preferences.mode.advances_automatically(),
        uses_audio: preferences.mode.uses_audio(),
    }
}

fn normalize_text_rate(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(
            MIN_TEXT_CHARACTERS_PER_SECOND,
            MAX_TEXT_CHARACTERS_PER_SECOND,
        )
    } else {
        ReadingPreferences::default().base_text_characters_per_second
    }
}

fn normalize_multiplier(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_PLAYBACK_RATE, MAX_PLAYBACK_RATE)
    } else {
        1.0
    }
}
