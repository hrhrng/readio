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
    next_sample: usize,
    pending_frames: u64,
    presented_frames: u64,
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
            next_sample: 0,
            pending_frames: 0,
            presented_frames: 0,
        }
    }

    /// Fill one device buffer without allocating.
    ///
    /// The previous submission is committed first: another callback means the
    /// device callback has crossed that period boundary. A paused callback submits only
    /// the zeroes already written below and leaves the PCM cursor untouched.
    fn fill(&mut self, output: &mut [f32], paused: bool) -> FrameReport {
        output.fill(0.0);
        self.presented_frames = self.presented_frames.saturating_add(self.pending_frames);
        self.pending_frames = 0;

        if !paused {
            let capacity = output.len() / self.channels;
            let available = self.samples.len().saturating_sub(self.next_sample) / self.channels;
            let submitted = capacity.min(available);
            let sample_count = submitted * self.channels;
            output[..sample_count]
                .copy_from_slice(&self.samples[self.next_sample..self.next_sample + sample_count]);
            self.next_sample += sample_count;
            self.pending_frames = submitted as u64;
        }

        FrameReport {
            presented_frames: self.presented_frames,
            submitted_frames: self.pending_frames,
            drained: self.next_sample == self.samples.len() && self.pending_frames == 0,
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
            let report = cursor.fill(output, silent);
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
    let mut decoder = DecoderBuilder::new_f32(info.channels as u32, sample_rate)
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

    /// The buffer submitted by callback N is only considered presented when
    /// callback N+1 arrives. This keeps the UI behind the callback boundary
    /// instead of highlighting audio that is merely queued for the device.
    #[test]
    fn presentation_lags_submission_by_one_device_buffer() {
        let mut cursor = PcmCursor::new(vec![0.1, 0.2, 0.3, 0.4], 1);
        let mut output = [0.0; 2];

        let first = cursor.fill(&mut output, false);
        assert_eq!(output, [0.1, 0.2]);
        assert_eq!(first.presented_frames, 0);
        assert_eq!(first.submitted_frames, 2);

        let paused = cursor.fill(&mut output, true);
        assert_eq!(output, [0.0, 0.0]);
        assert_eq!(paused.presented_frames, 2);
        assert_eq!(paused.submitted_frames, 0);

        let held = cursor.fill(&mut output, true);
        assert_eq!(held.presented_frames, 2);
        assert_eq!(held.submitted_frames, 0);

        let resumed = cursor.fill(&mut output, false);
        assert_eq!(output, [0.3, 0.4]);
        assert_eq!(resumed.presented_frames, 2);
        assert_eq!(resumed.submitted_frames, 2);

        let drained = cursor.fill(&mut output, false);
        assert_eq!(drained.presented_frames, 4);
        assert!(drained.drained);
    }
}
