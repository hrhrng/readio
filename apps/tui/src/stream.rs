//! Phrase-level streaming.
//!
//! Real token streams break Chinese in the middle of a word, which reads as
//! noise. Instead we split on clause boundaries and punctuation, then release
//! chunks against a characters-per-second budget with a short hold after each
//! sentence. The result looks like a model thinking in phrases rather than a
//! typewriter spraying bytes.

use std::collections::VecDeque;

use unicode_segmentation::UnicodeSegmentation;

/// One unit of released text plus the pause that follows it.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub pause_ms: u64,
}

/// Hard clause terminators: break after these, with a longer pause.
const SENTENCE_END: &str = "。！？…";
/// Soft separators: break after these, with a short pause.
const CLAUSE_END: &str = "，、；：》」』”’)]}";
/// Target column width of a phrase before we break it anyway.
const SOFT_LIMIT: usize = 18;

/// Split text into phrase chunks with per-chunk pauses.
pub fn split_phrases(text: &str) -> VecDeque<Chunk> {
    let mut out: VecDeque<Chunk> = VecDeque::new();
    for (i, para) in text.split('\n').enumerate() {
        if i > 0 {
            // Paragraph break: emitted as its own chunk so the reader sees the
            // blank line land before the next sentence starts.
            out.push_back(Chunk {
                text: "\n".to_string(),
                pause_ms: 140,
            });
        }
        if para.trim().is_empty() {
            continue;
        }
        let mut buf = String::new();
        let mut cols = 0usize;
        let mut graphemes = para.graphemes(true).peekable();

        while let Some(g) = graphemes.next() {
            buf.push_str(g);
            cols += crate::wrap::display_width(g);

            let sentence = SENTENCE_END.contains(g);
            let clause = CLAUSE_END.contains(g);
            let ascii_stop =
                matches!(g, "." | "!" | "?") && graphemes.peek().is_none_or(|n| *n == " ");
            let ascii_soft =
                matches!(g, "," | ";" | ":") && graphemes.peek().is_none_or(|n| *n == " ");
            let long_enough = cols >= SOFT_LIMIT && (g == " " || is_break_safe(g));

            if sentence || ascii_stop {
                out.push_back(Chunk {
                    text: std::mem::take(&mut buf),
                    pause_ms: 110,
                });
                cols = 0;
            } else if clause || ascii_soft {
                out.push_back(Chunk {
                    text: std::mem::take(&mut buf),
                    pause_ms: 45,
                });
                cols = 0;
            } else if long_enough {
                out.push_back(Chunk {
                    text: std::mem::take(&mut buf),
                    pause_ms: 0,
                });
                cols = 0;
            }
        }
        if !buf.is_empty() {
            out.push_back(Chunk {
                text: buf,
                pause_ms: 60,
            });
        }
    }
    out
}

/// A grapheme we are willing to break after when a phrase runs long.
fn is_break_safe(g: &str) -> bool {
    g.chars().next().is_some_and(|c| {
        // CJK ideographs and kana can break anywhere; Latin letters cannot.
        matches!(c as u32, 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF)
    })
}

/// Character-budget pacer shared by thinking and message streams.
#[derive(Debug)]
pub struct Pacer {
    /// Characters per second.
    pub cps: f32,
    /// Fractional characters carried between frames.
    credit: f32,
    /// Milliseconds of mandated silence still owed.
    hold_ms: f32,
}

impl Pacer {
    pub fn new(cps: f32) -> Self {
        Self {
            cps,
            credit: 0.0,
            hold_ms: 0.0,
        }
    }

    pub fn set_cps(&mut self, cps: f32) {
        self.cps = cps.clamp(4.0, 4000.0);
    }

    /// Advance the clock and drain whatever the budget allows.
    ///
    /// Returns the concatenated text released this frame. A phrase larger than
    /// the remaining budget is released in part, so a slow reading speed still
    /// produces smooth output instead of periodic bursts.
    ///
    /// `ceiling` is a hard limit on how many characters may leave this call,
    /// whatever the clock says — read-aloud uses it to stop the text at the
    /// sentence being spoken. A pacer held at its ceiling stops accumulating
    /// credit, so lifting the hold resumes the stream rather than firing off
    /// everything the wait was worth in a single frame.
    pub fn pump(
        &mut self,
        queue: &mut VecDeque<Chunk>,
        dt_ms: f32,
        ceiling: Option<usize>,
    ) -> String {
        if self.hold_ms > 0.0 {
            self.hold_ms -= dt_ms;
            if self.hold_ms > 0.0 {
                return String::new();
            }
        }
        self.credit += self.cps * dt_ms / 1000.0;
        let mut room = ceiling.unwrap_or(usize::MAX);
        if let Some(ceiling) = ceiling {
            self.credit = self.credit.min(ceiling as f32);
        }

        let mut released = String::new();
        while self.credit >= 1.0 && room > 0 {
            let Some(front) = queue.front_mut() else {
                break;
            };
            let cost = front.text.chars().count();

            if cost as f32 <= self.credit && cost <= room {
                let chunk = queue.pop_front().expect("front checked");
                self.credit -= cost as f32;
                room -= cost;
                released.push_str(&chunk.text);
                if chunk.pause_ms > 0 {
                    self.hold_ms = chunk.pause_ms as f32;
                    break;
                }
                continue;
            }

            // Partial release: hand over as many characters as the budget buys
            // and leave the remainder (and its pause) in the queue.
            let take = (self.credit.floor() as usize).min(room).min(cost);
            if take == 0 {
                break;
            }
            let taken: String = front.text.chars().take(take).collect();
            front.text = front.text.chars().skip(take).collect();
            self.credit -= take as f32;
            released.push_str(&taken);
            break;
        }
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(queue: &VecDeque<Chunk>) -> String {
        queue.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn splitting_loses_nothing() {
        let text = "第一句。第二句，带个逗号；还有分号。\n下一段开头。";
        let queue = split_phrases(text);
        assert_eq!(joined(&queue), text, "text must round-trip exactly");
    }

    #[test]
    fn breaks_after_sentence_punctuation() {
        let queue = split_phrases("界面不是中立的。它塑造你。");
        let first = queue.front().expect("a chunk");
        assert!(
            first.text.ends_with('。'),
            "first chunk should end at the sentence mark: {:?}",
            first.text
        );
        assert!(first.pause_ms >= 100, "sentences should hold longer");
    }

    #[test]
    fn never_emits_an_empty_chunk() {
        for text in ["", "。", "\n\n\n", "ok", "中文"] {
            for chunk in split_phrases(text) {
                assert!(!chunk.text.is_empty(), "empty chunk from {text:?}");
            }
        }
    }

    #[test]
    fn long_runs_are_broken_even_without_punctuation() {
        let text = "注意力".repeat(20);
        let queue = split_phrases(&text);
        assert!(queue.len() > 1, "a long unpunctuated run must still stream");
        for chunk in &queue {
            assert!(
                chunk.text.chars().count() <= SOFT_LIMIT,
                "chunk too long: {:?}",
                chunk.text
            );
        }
    }

    #[test]
    fn pacer_respects_the_character_budget() {
        let mut queue = split_phrases("一二三四五六七八九十");
        let mut pacer = Pacer::new(10.0);
        // 100ms at 10 chars/s buys one character.
        let first = pacer.pump(&mut queue, 100.0, None);
        assert!(
            first.chars().count() <= 2,
            "released too much too early: {first:?}"
        );
        assert!(!queue.is_empty(), "should not drain instantly");
    }

    #[test]
    fn pacer_holds_after_a_pause_then_continues() {
        let mut queue = split_phrases("第一句。第二句。");
        let mut pacer = Pacer::new(1_000.0);

        let first = pacer.pump(&mut queue, 16.0, None);
        assert!(first.ends_with('。'), "should stop at the pause: {first:?}");

        // Inside the hold window nothing comes out.
        assert_eq!(pacer.pump(&mut queue, 10.0, None), "");
        // Once it expires, the rest flows.
        let rest = pacer.pump(&mut queue, 200.0, None);
        assert!(rest.contains("第二句"), "stream stalled: {rest:?}");
    }

    #[test]
    fn pacer_drains_everything_eventually() {
        let text = "读书这件事，慢一点也没关系。真正折磨人的不是还剩很多，而是不知道还剩多少。";
        let mut queue = split_phrases(text);
        let mut pacer = Pacer::new(40.0);
        let mut out = String::new();
        for _ in 0..2_000 {
            out.push_str(&pacer.pump(&mut queue, 16.0, None));
            if queue.is_empty() {
                break;
            }
        }
        assert!(queue.is_empty(), "queue never drained");
        assert_eq!(out, text, "streamed text must match the source");
    }

    /// The ceiling is what keeps the text from running ahead of a voice reading
    /// it aloud: however fast the pacer is set, it may not release past it.
    #[test]
    fn a_ceiling_stops_the_stream_wherever_it_is_put() {
        let text = "第一句。第二句。第三句。";
        let mut queue = split_phrases(text);
        // Fast enough to drain the whole thing in one frame, were it allowed.
        let mut pacer = Pacer::new(4_000.0);
        let mut out = String::new();
        for _ in 0..50 {
            out.push_str(&pacer.pump(&mut queue, 16.0, Some(4 - out.chars().count())));
        }
        assert_eq!(out, "第一句。", "the ceiling is a hard stop, not a hint");
        assert!(!queue.is_empty(), "the rest must still be waiting");
    }

    /// A stream held at its ceiling must not save the wait up and spend it the
    /// moment the hold lifts. Before the credit was clamped, a sentence that
    /// spent three seconds waiting for its audio came out all at once.
    #[test]
    fn a_held_stream_does_not_burst_when_the_hold_lifts() {
        let mut queue = split_phrases("一二三四五六七八九十十一十二");
        let mut pacer = Pacer::new(50.0);
        for _ in 0..60 {
            assert_eq!(
                pacer.pump(&mut queue, 16.0, Some(0)),
                "",
                "nothing may leave while the ceiling is zero"
            );
        }
        // Nearly a second of waiting, worth 50 characters at this pace. One
        // frame after the hold lifts should still only be worth one frame.
        let burst = pacer.pump(&mut queue, 16.0, None);
        assert!(
            burst.chars().count() <= 2,
            "the wait was spent in one frame: {burst:?}"
        );
    }

    /// Lifting the ceiling entirely leaves the text exactly as it was written:
    /// the hold delays characters, it never drops them.
    #[test]
    fn a_ceiling_delays_text_without_losing_any_of_it() {
        let text = "读书这件事，慢一点也没关系。真正折磨人的不是还剩很多。";
        let mut queue = split_phrases(text);
        let mut pacer = Pacer::new(200.0);
        let mut out = String::new();
        for frame in 0..2_000 {
            // A ceiling that creeps up two characters every other frame, the
            // way a sentence of audio releases the text behind it.
            let ceiling = (frame / 2 * 2usize).saturating_sub(out.chars().count());
            out.push_str(&pacer.pump(&mut queue, 16.0, Some(ceiling)));
            if queue.is_empty() {
                break;
            }
        }
        assert!(queue.is_empty(), "queue never drained");
        assert_eq!(out, text, "a held stream must still be the same text");
    }
}
