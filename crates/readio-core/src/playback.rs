use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackPolicy {
    pub tick_interval_ms: u64,
    pub prefetch_window: usize,
}

impl Default for PlaybackPolicy {
    fn default() -> Self {
        Self {
            tick_interval_ms: 250,
            prefetch_window: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Failed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackState {
    pub status: PlaybackStatus,
    pub current_sentence: usize,
    pub sentence_count: usize,
    #[serde(default)]
    pub generation: u64,
    pub current_duration_ms: Option<u64>,
    pub current_position_ms: u64,
    pub last_tick_monotonic_ms: Option<u64>,
    pub sleep_deadline_unix_ms: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

impl PlaybackState {
    pub fn new(sentence_count: usize) -> Self {
        Self {
            status: PlaybackStatus::Idle,
            current_sentence: 0,
            sentence_count,
            generation: 0,
            current_duration_ms: None,
            current_position_ms: 0,
            last_tick_monotonic_ms: None,
            sleep_deadline_unix_ms: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchPriority {
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybackEvent {
    PlayRequested,
    AudioSourceChanged,
    SeekRequested {
        sentence: usize,
        position_ms: u64,
        autoplay: bool,
    },
    PauseRequested {
        audio_position_ms: u64,
    },
    AudioReady {
        sentence: usize,
        generation: u64,
        duration_ms: u64,
    },
    AudioFailed {
        sentence: usize,
        generation: u64,
        message: String,
    },
    ClockTick {
        generation: u64,
        monotonic_ms: u64,
        unix_ms: i64,
        audio_position_ms: u64,
    },
    SleepTimerSet {
        deadline_unix_ms: i64,
    },
    SleepTimerCleared,
    AudioEnded {
        sentence: usize,
        generation: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybackEffect {
    FetchAudio {
        sentence: usize,
        generation: u64,
        priority: FetchPriority,
    },
    PlayAudio {
        sentence: usize,
        generation: u64,
        offset_ms: u64,
    },
    ScheduleTick {
        generation: u64,
        after_ms: u64,
    },
    PauseAudio,
    PersistProgress {
        sentence: usize,
        position_ms: u64,
    },
    PrefetchAudio {
        sentences: Vec<usize>,
    },
    PlaybackFailed {
        sentence: usize,
        message: String,
    },
    PlaybackCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackTransition {
    pub state: PlaybackState,
    pub effects: Vec<PlaybackEffect>,
}

pub fn reduce(
    policy: &PlaybackPolicy,
    mut state: PlaybackState,
    event: PlaybackEvent,
) -> PlaybackTransition {
    match event {
        PlaybackEvent::PlayRequested => {
            if matches!(
                state.status,
                PlaybackStatus::Loading | PlaybackStatus::Playing
            ) {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            if state.sentence_count == 0 {
                state.status = PlaybackStatus::Completed;
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            if state.status == PlaybackStatus::Paused && state.current_duration_ms.is_some() {
                state.generation = state.generation.saturating_add(1);
                state.status = PlaybackStatus::Playing;
                let sentence = state.current_sentence;
                let offset_ms = state.current_position_ms;
                let generation = state.generation;
                return PlaybackTransition {
                    state,
                    effects: vec![
                        PlaybackEffect::PlayAudio {
                            sentence,
                            generation,
                            offset_ms,
                        },
                        PlaybackEffect::ScheduleTick {
                            generation,
                            after_ms: policy.tick_interval_ms,
                        },
                    ],
                };
            }
            state.generation = state.generation.saturating_add(1);
            state.status = PlaybackStatus::Loading;
            state.error = None;
            let sentence = state.current_sentence;
            let generation = state.generation;
            let prefetch_end = state.sentence_count.min(
                sentence
                    .saturating_add(1)
                    .saturating_add(policy.prefetch_window),
            );
            let prefetch: Vec<_> = (sentence + 1..prefetch_end).collect();
            let mut effects = vec![PlaybackEffect::FetchAudio {
                sentence,
                generation,
                priority: FetchPriority::User,
            }];
            if !prefetch.is_empty() {
                effects.push(PlaybackEffect::PrefetchAudio {
                    sentences: prefetch,
                });
            }
            PlaybackTransition { state, effects }
        }
        PlaybackEvent::AudioSourceChanged => {
            let was_active = matches!(
                state.status,
                PlaybackStatus::Loading | PlaybackStatus::Playing
            );
            state.generation = state.generation.saturating_add(1);
            state.current_duration_ms = None;
            state.last_tick_monotonic_ms = None;
            state.error = None;

            let mut effects = vec![PlaybackEffect::PauseAudio];
            if was_active && state.sentence_count > 0 {
                state.status = PlaybackStatus::Loading;
                effects.push(PlaybackEffect::FetchAudio {
                    sentence: state.current_sentence,
                    generation: state.generation,
                    priority: FetchPriority::User,
                });
            }

            PlaybackTransition { state, effects }
        }
        PlaybackEvent::SeekRequested {
            sentence,
            position_ms,
            autoplay,
        } => {
            state.generation = state.generation.saturating_add(1);
            if state.sentence_count == 0 {
                state.status = PlaybackStatus::Completed;
                state.current_sentence = 0;
                state.current_duration_ms = None;
                state.current_position_ms = 0;
                state.last_tick_monotonic_ms = None;
                return PlaybackTransition {
                    state,
                    effects: vec![PlaybackEffect::PauseAudio],
                };
            }

            let target = sentence.min(state.sentence_count - 1);
            let retained_duration = (target == state.current_sentence)
                .then_some(state.current_duration_ms)
                .flatten();
            state.current_sentence = target;
            state.current_duration_ms = retained_duration;
            state.current_position_ms =
                retained_duration.map_or(position_ms, |duration| position_ms.min(duration));
            state.last_tick_monotonic_ms = None;
            state.error = None;

            let mut effects = vec![
                PlaybackEffect::PauseAudio,
                PlaybackEffect::PersistProgress {
                    sentence: target,
                    position_ms: state.current_position_ms,
                },
            ];

            if autoplay {
                if retained_duration.is_some() {
                    state.status = PlaybackStatus::Playing;
                    effects.push(PlaybackEffect::PlayAudio {
                        sentence: target,
                        generation: state.generation,
                        offset_ms: state.current_position_ms,
                    });
                    effects.push(PlaybackEffect::ScheduleTick {
                        generation: state.generation,
                        after_ms: policy.tick_interval_ms,
                    });
                } else {
                    state.status = PlaybackStatus::Loading;
                    effects.push(PlaybackEffect::FetchAudio {
                        sentence: target,
                        generation: state.generation,
                        priority: FetchPriority::User,
                    });
                }
            } else {
                state.status = PlaybackStatus::Paused;
            }

            PlaybackTransition { state, effects }
        }
        PlaybackEvent::PauseRequested { audio_position_ms } => {
            if !matches!(
                state.status,
                PlaybackStatus::Loading | PlaybackStatus::Playing
            ) {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            let was_playing = state.status == PlaybackStatus::Playing;
            state.generation = state.generation.saturating_add(1);
            state.status = PlaybackStatus::Paused;
            if was_playing {
                state.current_position_ms = state
                    .current_duration_ms
                    .map_or(audio_position_ms, |duration| {
                        audio_position_ms.min(duration)
                    });
            }
            state.last_tick_monotonic_ms = None;
            let sentence = state.current_sentence;
            let position_ms = state.current_position_ms;
            PlaybackTransition {
                state,
                effects: vec![
                    PlaybackEffect::PauseAudio,
                    PlaybackEffect::PersistProgress {
                        sentence,
                        position_ms,
                    },
                ],
            }
        }
        PlaybackEvent::AudioReady {
            sentence,
            generation,
            duration_ms,
        } => {
            if generation != state.generation
                || sentence != state.current_sentence
                || state.status != PlaybackStatus::Loading
            {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            state.status = PlaybackStatus::Playing;
            state.current_duration_ms = Some(duration_ms);
            state.current_position_ms = state.current_position_ms.min(duration_ms);
            state.last_tick_monotonic_ms = None;
            state.error = None;
            let offset_ms = state.current_position_ms;
            PlaybackTransition {
                state,
                effects: vec![
                    PlaybackEffect::PlayAudio {
                        sentence,
                        generation,
                        offset_ms,
                    },
                    PlaybackEffect::ScheduleTick {
                        generation,
                        after_ms: policy.tick_interval_ms,
                    },
                ],
            }
        }
        PlaybackEvent::AudioFailed {
            sentence,
            generation,
            message,
        } => {
            if generation != state.generation
                || sentence != state.current_sentence
                || !matches!(
                    state.status,
                    PlaybackStatus::Loading | PlaybackStatus::Playing
                )
            {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }

            state.generation = state.generation.saturating_add(1);
            state.status = PlaybackStatus::Failed;
            state.last_tick_monotonic_ms = None;
            state.error = Some(message.clone());
            PlaybackTransition {
                state,
                effects: vec![
                    PlaybackEffect::PauseAudio,
                    PlaybackEffect::PlaybackFailed { sentence, message },
                ],
            }
        }
        PlaybackEvent::ClockTick {
            generation,
            monotonic_ms,
            unix_ms,
            audio_position_ms,
        } => {
            if generation != state.generation || state.status != PlaybackStatus::Playing {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }
            state.current_position_ms = state
                .current_duration_ms
                .map_or(audio_position_ms, |duration| {
                    audio_position_ms.min(duration)
                });
            state.last_tick_monotonic_ms = Some(monotonic_ms);
            if state
                .sleep_deadline_unix_ms
                .is_some_and(|deadline| unix_ms >= deadline)
            {
                state.status = PlaybackStatus::Paused;
                state.sleep_deadline_unix_ms = None;
                let sentence = state.current_sentence;
                let position_ms = state.current_position_ms;
                return PlaybackTransition {
                    state,
                    effects: vec![
                        PlaybackEffect::PauseAudio,
                        PlaybackEffect::PersistProgress {
                            sentence,
                            position_ms,
                        },
                    ],
                };
            }
            PlaybackTransition {
                state,
                effects: vec![PlaybackEffect::ScheduleTick {
                    generation,
                    after_ms: policy.tick_interval_ms,
                }],
            }
        }
        PlaybackEvent::SleepTimerSet { deadline_unix_ms } => {
            state.sleep_deadline_unix_ms = Some(deadline_unix_ms);
            PlaybackTransition {
                state,
                effects: Vec::new(),
            }
        }
        PlaybackEvent::SleepTimerCleared => {
            state.sleep_deadline_unix_ms = None;
            PlaybackTransition {
                state,
                effects: Vec::new(),
            }
        }
        PlaybackEvent::AudioEnded {
            sentence,
            generation,
        } => {
            if generation != state.generation
                || sentence != state.current_sentence
                || state.status != PlaybackStatus::Playing
            {
                return PlaybackTransition {
                    state,
                    effects: Vec::new(),
                };
            }

            if sentence + 1 >= state.sentence_count {
                state.status = PlaybackStatus::Completed;
                state.current_position_ms = state
                    .current_duration_ms
                    .unwrap_or(state.current_position_ms);
                state.last_tick_monotonic_ms = None;
                let position_ms = state.current_position_ms;
                return PlaybackTransition {
                    state,
                    effects: vec![
                        PlaybackEffect::PersistProgress {
                            sentence,
                            position_ms,
                        },
                        PlaybackEffect::PlaybackCompleted,
                    ],
                };
            }

            state.current_sentence += 1;
            state.generation = state.generation.saturating_add(1);
            state.status = PlaybackStatus::Loading;
            state.current_duration_ms = None;
            state.current_position_ms = 0;
            state.last_tick_monotonic_ms = None;

            let current = state.current_sentence;
            let generation = state.generation;
            let prefetch_end = state.sentence_count.min(
                current
                    .saturating_add(1)
                    .saturating_add(policy.prefetch_window),
            );
            let prefetch = (current + 1..prefetch_end).collect();

            PlaybackTransition {
                state,
                effects: vec![
                    PlaybackEffect::PersistProgress {
                        sentence: current,
                        position_ms: 0,
                    },
                    PlaybackEffect::FetchAudio {
                        sentence: current,
                        generation,
                        priority: FetchPriority::User,
                    },
                    PlaybackEffect::PrefetchAudio {
                        sentences: prefetch,
                    },
                ],
            }
        }
    }
}
