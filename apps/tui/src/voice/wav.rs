//! Just enough WAV parsing to learn how long a clip is.
//!
//! The duration is what lets the text follow the audio: characters per second
//! becomes `chars / clip_seconds`, so a sentence finishes on screen exactly
//! when the speaker stops saying it.

use anyhow::{Result, anyhow};

/// Audio format facts we care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data_bytes: u32,
}

impl Info {
    pub fn ms(&self) -> u64 {
        let bytes_per_frame = (self.channels as u32) * (self.bits_per_sample as u32 / 8);
        if bytes_per_frame == 0 || self.sample_rate == 0 {
            return 0;
        }
        let frames = self.data_bytes / bytes_per_frame;
        (frames as u64 * 1000) / self.sample_rate as u64
    }
}

/// Read the header of a RIFF/WAVE file.
///
/// Chunk walking rather than fixed offsets: engines emit `LIST`, `fact` and
/// other chunks before `data`, and ONNX exporters like odd `fmt ` sizes.
pub fn info(bytes: &[u8]) -> Result<Info> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(anyhow!("not a RIFF/WAVE file"));
    }

    let mut cursor = 12usize;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    let mut data_bytes = None;

    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        let body = cursor + 8;

        match id {
            b"fmt " if body + 16 <= bytes.len() => {
                channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
                sample_rate = u32::from_le_bytes([
                    bytes[body + 4],
                    bytes[body + 5],
                    bytes[body + 6],
                    bytes[body + 7],
                ]);
                bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            }
            b"data" => {
                // A streaming writer may leave the size at 0 or 0xFFFFFFFF;
                // trust the file length instead.
                let declared = size;
                let actual = bytes.len().saturating_sub(body);
                let usable = if declared == 0 || declared > actual {
                    actual
                } else {
                    declared
                };
                data_bytes = Some(usable as u32);
                break;
            }
            _ => {}
        }

        // Chunks are word-aligned.
        cursor = body + size + (size % 2);
    }

    let data_bytes = data_bytes.ok_or_else(|| anyhow!("WAVE file has no data chunk"))?;
    if sample_rate == 0 || channels == 0 || bits == 0 {
        return Err(anyhow!("WAVE file has no usable fmt chunk"));
    }
    Ok(Info {
        sample_rate,
        channels,
        bits_per_sample: bits,
        data_bytes,
    })
}

/// Duration of a clip on disk, in milliseconds.
pub fn duration_ms(path: &std::path::Path) -> Result<u64> {
    let bytes = std::fs::read(path)?;
    Ok(info(&bytes)?.ms())
}

/// Build a minimal WAV file of `ms` silence. Used by tests and `/voice test`
/// fallbacks so playback paths can be exercised without a model.
pub fn silence(ms: u64, sample_rate: u32) -> Vec<u8> {
    let frames = (sample_rate as u64 * ms / 1000) as u32;
    let data_bytes = frames * 2;
    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    out.resize(44 + data_bytes as usize, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plain_pcm_header() {
        let bytes = silence(1_500, 24_000);
        let info = info(&bytes).expect("header");
        assert_eq!(info.sample_rate, 24_000);
        assert_eq!(info.channels, 1);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.ms(), 1_500);
    }

    #[test]
    fn walks_past_extra_chunks() {
        let mut bytes = silence(500, 22_050);
        // Splice a LIST chunk between `fmt ` and `data`, as some engines do.
        let data_at = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("data chunk");
        let mut spliced = bytes[..data_at].to_vec();
        spliced.extend_from_slice(b"LIST");
        spliced.extend_from_slice(&6u32.to_le_bytes());
        spliced.extend_from_slice(b"INFOxy");
        spliced.extend_from_slice(&bytes[data_at..]);
        bytes = spliced;

        let info = info(&bytes).expect("header");
        assert_eq!(info.ms(), 500, "duration should survive an extra chunk");
    }

    #[test]
    fn tolerates_a_streaming_writers_zero_length() {
        let mut bytes = silence(800, 16_000);
        let data_at = bytes.windows(4).position(|w| w == b"data").unwrap();
        // Zero out the declared data size.
        bytes[data_at + 4..data_at + 8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(info(&bytes).expect("header").ms(), 800);
    }

    #[test]
    fn stereo_and_24_bit_are_measured_correctly() {
        let mut bytes = silence(1_000, 48_000);
        let fmt_at = bytes.windows(4).position(|w| w == b"fmt ").unwrap() + 8;
        bytes[fmt_at + 2..fmt_at + 4].copy_from_slice(&2u16.to_le_bytes()); // stereo
        bytes[fmt_at + 14..fmt_at + 16].copy_from_slice(&24u16.to_le_bytes()); // 24-bit
        let info = info(&bytes).expect("header");
        // Same bytes, but each frame is now 6 bytes instead of 2.
        assert_eq!(info.ms(), 1_000 / 3);
    }

    #[test]
    fn rejects_things_that_are_not_wav() {
        assert!(info(b"").is_err());
        assert!(info(b"not audio at all").is_err());
        let mut headerless = b"RIFF____WAVE".to_vec();
        headerless.extend_from_slice(b"junk");
        headerless.extend_from_slice(&0u32.to_le_bytes());
        assert!(
            info(&headerless).is_err(),
            "no data chunk means no duration"
        );
    }
}
