//! Embedded PCM output.
//!
//! TTS clips are short, so they are decoded before the device starts. The
//! real-time callback only clears and copies an existing buffer, then publishes
//! atomics; it never allocates, locks, logs, or touches the filesystem.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use maudio::{
    audio::sample_rate::SampleRate,
    backend::Backend,
    data_source::sources::decoder::{DecoderBuilder, DecoderOps},
    device::device_builder::{DeviceBuilder, DeviceBuilderOps},
};

use super::PlaybackControl;

pub(super) const INTERNAL_PLAYER: &str = "@readio";

#[derive(Clone)]
struct PcmCursor {
    samples: Vec<f32>,
    channels: usize,
    next_frame: f64,
    pending_frame: Option<f64>,
    presented_frame: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameReport {
    presented_frames: u64,
    submitted_frames: u64,
    drained: bool,
}

struct Decoded {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

struct CallbackState {
    seen: AtomicBool,
    paused: AtomicBool,
    cancelled: AtomicBool,
    drained: AtomicBool,
    presented: AtomicU64,
}

impl Default for CallbackState {
    fn default() -> Self {
        Self {
            seen: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            drained: AtomicBool::new(false),
            presented: AtomicU64::new(0),
        }
    }
}

impl PcmCursor {
    fn new(samples: Vec<f32>, channels: usize) -> Self {
        Self {
            samples,
            channels: channels.max(1),
            next_frame: 0.0,
            pending_frame: None,
            presented_frame: 0.0,
        }
    }

    /// Fill one device buffer without allocating.
    ///
    /// The previous submission is committed first: another callback means the
    /// device callback has crossed that period boundary. A paused callback submits only
    /// the zeroes already written below and leaves the PCM cursor untouched.
    fn fill(&mut self, output: &mut [f32], paused: bool, rate: f32) -> FrameReport {
        output.fill(0.0);
        if let Some(pending) = self.pending_frame.take() {
            self.presented_frame = pending;
        }

        let total_frames = self.samples.len() / self.channels;
        let mut submitted = 0usize;
        if !paused {
            let capacity = output.len() / self.channels;
            let step = rate.clamp(0.5, 3.0) as f64;
            while submitted < capacity && self.next_frame < total_frames as f64 {
                let at = self.next_frame.floor() as usize;
                let next = (at + 1).min(total_frames.saturating_sub(1));
                let fraction = (self.next_frame - at as f64) as f32;
                for channel in 0..self.channels {
                    let a = self.samples[at * self.channels + channel];
                    let b = self.samples[next * self.channels + channel];
                    output[submitted * self.channels + channel] = a + (b - a) * fraction;
                }
                submitted += 1;
                self.next_frame = (self.next_frame + step).min(total_frames as f64);
            }
            if submitted > 0 {
                self.pending_frame = Some(self.next_frame);
            }
        }

        FrameReport {
            presented_frames: self.presented_frame.floor() as u64,
            submitted_frames: submitted as u64,
            drained: self.next_frame >= total_frames as f64 && self.pending_frame.is_none(),
        }
    }
}

pub(super) fn play(path: &std::path::Path, control: &PlaybackControl) -> Result<()> {
    play_with_backends(path, control, &[])
}

fn play_with_backends(
    path: &std::path::Path,
    control: &PlaybackControl,
    backends: &[Backend],
) -> Result<()> {
    let decoded = decode(path)?;
    let total_frames = (decoded.samples.len() / decoded.channels) as u64;
    let sample_rate =
        SampleRate::try_from(decoded.sample_rate).map_err(|err| anyhow!("{err:?}"))?;

    let state = Arc::new(CallbackState::default());
    let callback_state = Arc::clone(&state);
    let requested_pause = Arc::clone(&control.paused);
    let requested_rate = Arc::clone(&control.rate);
    let mut cursor = PcmCursor::new(decoded.samples, decoded.channels);
    let mut untyped = DeviceBuilder::playback();
    let mut builder = untyped.f32();
    builder
        .playback_channels(decoded.channels.try_into().unwrap_or(u32::MAX))
        .sample_rate(sample_rate)
        // Ten milliseconds bounds pause acknowledgement without making the
        // audio thread churn on tiny buffers.
        .period_size_millis(10);
    if !backends.is_empty() {
        builder.backends(backends);
    }
    let mut device = builder
        .with_callback(move |_device, output| {
            let paused = requested_pause.load(Ordering::SeqCst);
            let silent = paused || callback_state.cancelled.load(Ordering::SeqCst);
            let report = cursor.fill(output, silent, super::playback_rate(&requested_rate));
            callback_state
                .presented
                .store(report.presented_frames, Ordering::Release);
            callback_state.paused.store(paused, Ordering::Release);
            callback_state.seen.store(true, Ordering::Release);
            if report.drained || callback_state.cancelled.load(Ordering::SeqCst) {
                callback_state.drained.store(true, Ordering::Release);
            }
        })
        .map_err(|err| anyhow!("audio output device: {err:?}"))?;
    device
        .device_start()
        .map_err(|err| anyhow!("start audio output: {err:?}"))?;

    let mut started = false;
    let mut acknowledged_pause = control.paused();
    loop {
        if control.cancelled() {
            state.cancelled.store(true, Ordering::SeqCst);
        }
        if state.seen.load(Ordering::Acquire) && !started {
            control.started_frames(total_frames, decoded.sample_rate);
            acknowledged_pause = state.paused.load(Ordering::Acquire);
            started = true;
        }
        if started {
            control.presented_frames(state.presented.load(Ordering::Acquire));
            let actual_pause = state.paused.load(Ordering::Acquire);
            if actual_pause != acknowledged_pause {
                if actual_pause {
                    control.pause_applied();
                } else {
                    control.resume_applied();
                }
                acknowledged_pause = actual_pause;
            }
        }
        if state.drained.load(Ordering::Acquire) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    device
        .device_stop()
        .map_err(|err| anyhow!("stop audio output: {err:?}"))
}

fn decode(path: &std::path::Path) -> Result<Decoded> {
    let bytes = std::fs::read(path).with_context(|| format!("read audio {}", path.display()))?;
    let info = super::wav::info(&bytes)?;
    let channels = info.channels as usize;
    let sample_rate = SampleRate::try_from(info.sample_rate).map_err(|err| anyhow!("{err:?}"))?;
    let mut decoder = DecoderBuilder::new_f32()
        .channels(info.channels as u32)
        .sample_rate(sample_rate)
        .copy_memory(Arc::<[u8]>::from(bytes))
        .map_err(|err| anyhow!("decode wav {}: {err:?}", path.display()))?;
    let capacity = usize::try_from(info.frames())
        .ok()
        .and_then(|frames| frames.checked_mul(channels))
        .ok_or_else(|| anyhow!("audio clip is too large"))?;
    let mut samples = vec![0.0f32; capacity];
    let read = decoder
        .read_pcm_frames_into(&mut samples)
        .map_err(|err| anyhow!("decode wav frames {}: {err:?}", path.display()))?;
    samples.truncate(read.saturating_mul(channels));
    if samples.is_empty() {
        return Err(anyhow!("audio clip has no PCM frames"));
    }
    Ok(Decoded {
        samples,
        channels,
        sample_rate: info.sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    use crate::voice::{Pipeline, PlaybackClock};

    /// A started device is not enough: the callback must consume the clip and
    /// let the blocking playback call return. This exercises the same default
    /// backend as a real read-aloud session, because a device that reports a
    /// successful start without invoking its callback otherwise leaves the
    /// player thread asleep forever.
    #[cfg(target_os = "macos")]
    #[test]
    fn default_output_consumes_a_short_pcm_clip() {
        let path =
            std::env::temp_dir().join(format!("readio-output-{}-short.wav", std::process::id()));
        std::fs::write(&path, crate::voice::wav::silence(80, 48_000)).expect("write test wav");

        let pipeline = Arc::new(Pipeline::new(1));
        let clock = Arc::new(PlaybackClock::default());
        let (events, _event_rx) = mpsc::channel();
        let control = PlaybackControl {
            pipeline,
            era: 0,
            paused: Arc::new(AtomicBool::new(false)),
            rate: Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits())),
            clock,
            events,
            id: 1,
            range: (0, 1),
            chars: 1,
            ms: 80,
        };
        let (finished, result) = mpsc::channel();
        let played = path.clone();
        std::thread::spawn(move || {
            let _ = finished.send(play(&played, &control));
        });

        let outcome = result
            .recv_timeout(Duration::from_secs(2))
            .expect("the audio device started but its callback never consumed the clip");
        outcome.expect("play short PCM clip");
        let _ = std::fs::remove_file(path);
    }

    /// The embedded transport consumes canonical PCM at the selected rate; it
    /// never asks the synthesizer for another version of the clip.
    #[test]
    fn pcm_output_applies_playback_rate() {
        let path =
            std::env::temp_dir().join(format!("readio-output-{}-rate.wav", std::process::id()));
        std::fs::write(&path, crate::voice::wav::silence(800, 48_000)).expect("write test wav");

        let pipeline = Arc::new(Pipeline::new(1));
        let clock = Arc::new(PlaybackClock::default());
        let (events, event_rx) = mpsc::channel();
        let control = PlaybackControl {
            pipeline,
            era: 0,
            paused: Arc::new(AtomicBool::new(false)),
            rate: Arc::new(std::sync::atomic::AtomicU32::new(2.0f32.to_bits())),
            clock,
            events,
            id: 1,
            range: (0, 1),
            chars: 1,
            ms: 800,
        };
        let (finished, result) = mpsc::channel();
        let played = path.clone();
        std::thread::spawn(move || {
            let outcome = play_with_backends(&played, &control, &[Backend::Null]);
            let _ = finished.send(outcome);
        });

        event_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("PCM playback never started");
        let started = std::time::Instant::now();
        result
            .recv_timeout(Duration::from_millis(600))
            .expect("2× PCM playback still consumed the clip at 1×")
            .expect("play rate-adjusted PCM clip");
        assert!(
            started.elapsed() < Duration::from_millis(600),
            "800ms of media did not finish inside the 2× playback window"
        );
        let _ = std::fs::remove_file(path);
    }

    /// The buffer submitted by callback N is only considered presented when
    /// callback N+1 arrives. This keeps the UI behind the callback boundary
    /// instead of highlighting audio that is merely queued for the device.
    #[test]
    fn presentation_lags_submission_by_one_device_buffer() {
        let mut cursor = PcmCursor::new(vec![0.1, 0.2, 0.3, 0.4], 1);
        let mut output = [0.0; 2];

        let first = cursor.fill(&mut output, false, 1.0);
        assert_eq!(output, [0.1, 0.2]);
        assert_eq!(first.presented_frames, 0);
        assert_eq!(first.submitted_frames, 2);

        let paused = cursor.fill(&mut output, true, 1.0);
        assert_eq!(output, [0.0, 0.0]);
        assert_eq!(paused.presented_frames, 2);
        assert_eq!(paused.submitted_frames, 0);

        let held = cursor.fill(&mut output, true, 1.0);
        assert_eq!(held.presented_frames, 2);
        assert_eq!(held.submitted_frames, 0);

        let resumed = cursor.fill(&mut output, false, 1.0);
        assert_eq!(output, [0.3, 0.4]);
        assert_eq!(resumed.presented_frames, 2);
        assert_eq!(resumed.submitted_frames, 2);

        let drained = cursor.fill(&mut output, false, 1.0);
        assert_eq!(drained.presented_frames, 4);
        assert!(drained.drained);
    }
}
