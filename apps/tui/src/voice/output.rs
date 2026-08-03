//! Embedded PCM output.
//!
//! TTS clips are short, so they are decoded before the device starts. The
//! real-time callback pulls them through a preallocated Sonic stream, copies
//! the result, then publishes atomics; it never allocates, locks, logs, or
//! touches the filesystem.

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

struct PcmCursor {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    source_frame: usize,
    media_frame: f64,
    pending_frame: Option<f64>,
    presented_frame: f64,
    sonic_active: bool,
    flushed: bool,
    ledger: RateLedger,
    sonic: Sonic,
}

#[derive(Debug, Clone, Copy, Default)]
struct RateSpan {
    frames: usize,
    rate: f64,
}

/// Fixed-capacity accounting for output Sonic has produced but the device has
/// not consumed yet. A live speed change does not rewrite those samples: they
/// retain the source-time rate at which Sonic made them.
#[derive(Default)]
struct RateLedger {
    spans: [RateSpan; 32],
    len: usize,
}

impl RateLedger {
    fn push(&mut self, frames: usize, rate: f32) {
        if frames == 0 {
            return;
        }
        let rate = rate as f64;
        if let Some(last) = self.spans.get_mut(self.len.saturating_sub(1))
            && self.len > 0
            && (last.rate - rate).abs() < f64::EPSILON
        {
            last.frames = last.frames.saturating_add(frames);
            return;
        }
        if self.len == self.spans.len() {
            let first = self.spans[0];
            let second = self.spans[1];
            let total = first.frames.saturating_add(second.frames);
            self.spans[1] = RateSpan {
                frames: total,
                rate: if total == 0 {
                    rate
                } else {
                    (first.rate * first.frames as f64 + second.rate * second.frames as f64)
                        / total as f64
                },
            };
            self.spans.copy_within(1..self.len, 0);
            self.len -= 1;
        }
        self.spans[self.len] = RateSpan { frames, rate };
        self.len += 1;
    }

    fn consume(&mut self, mut frames: usize, fallback: f32) -> f64 {
        let mut source = 0.0;
        while frames > 0 && self.len > 0 {
            let take = frames.min(self.spans[0].frames);
            source += take as f64 * self.spans[0].rate;
            frames -= take;
            self.spans[0].frames -= take;
            if self.spans[0].frames == 0 {
                self.spans.copy_within(1..self.len, 0);
                self.len -= 1;
            }
        }
        source + frames as f64 * fallback as f64
    }
}

/// Exclusive owner of one libsonic stream.
///
/// The C library keeps all mutable state behind this pointer. The wrapper is
/// moved once into the device callback and never shared, which is the ownership
/// contract libsonic requires.
struct Sonic {
    raw: sonic_rs_sys::sonicStream,
}

// SAFETY: `Sonic` owns the stream, exposes no aliases, and all access requires
// `&mut self`. Moving that exclusive owner to the audio thread is safe.
unsafe impl Send for Sonic {}

impl Drop for Sonic {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `sonicCreateStream`, remains exclusively
        // owned by this wrapper, and is destroyed exactly once here.
        unsafe { sonic_rs_sys::sonicDestroyStream(self.raw) }
    }
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
    fn new(samples: Vec<f32>, channels: usize, sample_rate: u32) -> Result<Self> {
        let channels = channels.max(1);
        let sonic = Sonic::new(sample_rate, channels)?;
        Ok(Self {
            samples,
            channels,
            sample_rate,
            source_frame: 0,
            media_frame: 0.0,
            pending_frame: None,
            presented_frame: 0.0,
            sonic_active: false,
            flushed: false,
            ledger: RateLedger::default(),
            sonic,
        })
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
            let rate = rate.clamp(0.5, 3.0);
            if !self.sonic_active && (rate - 1.0).abs() < f32::EPSILON {
                // Unity playback stays bit-for-bit PCM. If the reader changes
                // speed later, Sonic starts at this exact source frame.
                submitted = capacity.min(total_frames.saturating_sub(self.source_frame));
                let start = self.source_frame * self.channels;
                let end = start + submitted * self.channels;
                output[..submitted * self.channels].copy_from_slice(&self.samples[start..end]);
                self.source_frame += submitted;
                self.media_frame = self.source_frame as f64;
            } else {
                self.sonic_active = true;
                self.sonic.set_speed(rate);
                while submitted < capacity {
                    let destination = &mut output[submitted * self.channels..];
                    let read = self.sonic.read(destination, capacity - submitted);
                    if read > 0 {
                        submitted += read;
                        self.media_frame = (self.media_frame + self.ledger.consume(read, rate))
                            .min(total_frames as f64);
                        continue;
                    }

                    if self.source_frame < total_frames {
                        // Ten milliseconds keeps the processor just ahead of
                        // the device without turning a live rate change into a
                        // long queue of audio rendered at the old speed.
                        let feed_frames = (self.sample_rate as usize / 100).max(1);
                        let end = (self.source_frame + feed_frames).min(total_frames);
                        let samples =
                            &self.samples[self.source_frame * self.channels..end * self.channels];
                        let available = self.sonic.available();
                        if !self.sonic.write(samples, end - self.source_frame) {
                            self.flushed = true;
                            self.source_frame = total_frames;
                            break;
                        }
                        self.ledger
                            .push(self.sonic.available().saturating_sub(available), rate);
                        self.source_frame = end;
                        continue;
                    }

                    if !self.flushed {
                        self.flushed = true;
                        let available = self.sonic.available();
                        if !self.sonic.flush() {
                            break;
                        }
                        self.ledger
                            .push(self.sonic.available().saturating_sub(available), rate);
                        continue;
                    }
                    break;
                }
                if submitted > 0 && self.flushed && self.sonic.available() == 0 {
                    self.media_frame = total_frames as f64;
                }
            }
            if submitted > 0 {
                self.pending_frame = Some(self.media_frame);
            }
        }

        let drained = if self.sonic_active {
            self.flushed && self.sonic.available() == 0 && self.pending_frame.is_none()
        } else {
            self.source_frame >= total_frames && self.pending_frame.is_none()
        };

        FrameReport {
            presented_frames: self.presented_frame.floor() as u64,
            submitted_frames: submitted as u64,
            drained,
        }
    }
}

impl Sonic {
    fn new(sample_rate: u32, channels: usize) -> Result<Self> {
        let sample_rate = i32::try_from(sample_rate).context("audio sample rate is too large")?;
        let channels = i32::try_from(channels).context("audio channel count is too large")?;
        // SAFETY: scalar arguments satisfy libsonic's documented positive
        // ranges. Null is handled as an ordinary construction failure.
        let raw = unsafe { sonic_rs_sys::sonicCreateStream(sample_rate, channels) };
        if raw.is_null() {
            return Err(anyhow!("create pitch-preserving audio stream"));
        }
        let mut stream = Self { raw };
        stream.prime(sample_rate as u32, channels as usize)?;
        Ok(stream)
    }

    /// Grow libsonic's internal buffers before the device starts. Sonic keeps
    /// those capacities after a flush, so subsequent bounded writes cannot
    /// call `realloc` from the real-time callback.
    fn prime(&mut self, sample_rate: u32, channels: usize) -> Result<()> {
        let frames = (sample_rate as usize / 10).max(1);
        let mut silence = vec![0.0; frames * channels];
        self.set_speed(0.5);
        if !self.write(&silence, frames) || !self.flush() {
            return Err(anyhow!("prime pitch-preserving audio stream"));
        }
        while self.available() > 0 {
            let capacity = silence.len() / channels;
            let _ = self.read(&mut silence, capacity);
        }
        self.set_speed(1.0);
        Ok(())
    }

    fn set_speed(&mut self, speed: f32) {
        // SAFETY: `raw` is a live, exclusively owned stream.
        unsafe { sonic_rs_sys::sonicSetSpeed(self.raw, speed) }
    }

    fn write(&mut self, samples: &[f32], frames: usize) -> bool {
        let Ok(frames) = i32::try_from(frames) else {
            return false;
        };
        // SAFETY: the interleaved slice contains at least `frames * channels`
        // samples, and libsonic only reads it during this call.
        unsafe { sonic_rs_sys::sonicWriteFloatToStream(self.raw, samples.as_ptr(), frames) != 0 }
    }

    fn read(&mut self, output: &mut [f32], frames: usize) -> usize {
        let frames = i32::try_from(frames).unwrap_or(i32::MAX);
        // SAFETY: the caller provides space for the requested interleaved
        // frames, and libsonic returns no more than that request.
        unsafe { sonic_rs_sys::sonicReadFloatFromStream(self.raw, output.as_mut_ptr(), frames) }
            .max(0) as usize
    }

    fn flush(&mut self) -> bool {
        // SAFETY: `raw` is a live, exclusively owned stream.
        unsafe { sonic_rs_sys::sonicFlushStream(self.raw) != 0 }
    }

    fn available(&self) -> usize {
        // SAFETY: this read-only query accepts a live stream and returns the
        // number of complete frames currently buffered.
        unsafe { sonic_rs_sys::sonicSamplesAvailable(self.raw) }.max(0) as usize
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
    let mut cursor = PcmCursor::new(decoded.samples, decoded.channels, decoded.sample_rate)?;
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
    use std::f32::consts::TAU;
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
        let mut cursor =
            PcmCursor::new(vec![0.1, 0.2, 0.3, 0.4], 1, 48_000).expect("pitch-preserving cursor");
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

    fn rendered_pcm(samples: Vec<f32>, rate: f32) -> Vec<f32> {
        let mut cursor = PcmCursor::new(samples, 1, 48_000).expect("pitch-preserving cursor");
        let mut rendered = Vec::new();
        for _ in 0..20_000 {
            let mut buffer = [0.0; 256];
            let report = cursor.fill(&mut buffer, false, rate);
            rendered.extend_from_slice(&buffer[..report.submitted_frames as usize]);
            if report.drained {
                return rendered;
            }
        }
        panic!("PCM cursor did not drain");
    }

    fn positive_crossing_hz(samples: &[f32], sample_rate: u32) -> f32 {
        // Ignore both ends, where a time stretcher is allowed to prime and
        // drain its analysis window.
        let middle = &samples[samples.len() / 10..samples.len() * 9 / 10];
        let crossings = middle
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        crossings as f32 * sample_rate as f32 / middle.len() as f32
    }

    /// Replacing time stretching with resampling makes this fail at roughly
    /// `tone_hz * rate`, which is the sharp voice heard in real playback.
    #[test]
    fn playback_rate_preserves_pitch() {
        const SAMPLE_RATE: u32 = 48_000;
        const TONE_HZ: f32 = 440.0;
        let tone = (0..SAMPLE_RATE)
            .map(|frame| (TAU * TONE_HZ * frame as f32 / SAMPLE_RATE as f32).sin() * 0.5)
            .collect::<Vec<_>>();

        for rate in [0.75, 1.5, 2.0] {
            let rendered = rendered_pcm(tone.clone(), rate);
            let measured = positive_crossing_hz(&rendered, SAMPLE_RATE);
            assert!(
                (measured - TONE_HZ).abs() < 20.0,
                "{rate}× playback shifted a {TONE_HZ}Hz tone to {measured:.1}Hz"
            );
        }
    }

    /// The media clock must describe the samples actually handed to the device,
    /// including the callback immediately after a live rate change. Sonic can
    /// have output buffered at the old speed; counting those samples at the new
    /// speed makes the highlight lag or lead the voice even though both clocks
    /// remain individually monotonic.
    #[test]
    fn live_rate_change_keeps_pcm_content_and_reported_position_together() {
        const FRAMES: usize = 48_000;
        let samples = (0..FRAMES)
            .map(|frame| frame as f32 / FRAMES as f32)
            .collect::<Vec<_>>();
        let mut cursor = PcmCursor::new(samples, 1, 48_000).expect("pitch-preserving cursor");
        let mut output = vec![0.0; 128];

        // Small device periods leave enough old-rate output inside Sonic to
        // exercise the transition instead of changing speed at an empty edge.
        for _ in 0..24 {
            cursor.fill(&mut output, false, 3.0);
        }
        let changed = cursor.fill(&mut output, false, 0.5);
        assert!(
            changed.submitted_frames > 0,
            "the transition produced silence"
        );
        let audible_end =
            (output[changed.submitted_frames as usize - 1] * FRAMES as f32).round() as u64;

        // A silent callback commits the buffer above without consuming another
        // source frame, giving the position that the app would project to text.
        let committed = cursor.fill(&mut output, true, 0.5);
        let error = committed.presented_frames.abs_diff(audible_end);
        assert!(
            error <= 96,
            "PTS is {error} source frames away from the audio content after a live rate change"
        );
    }
}
