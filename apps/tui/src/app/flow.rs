//! Reading logic: turning book positions and user intent into turn steps.
//!
//! This is where the disguise is decided. A reading action is expressed the way
//! a coding agent would express it — a thought, a tool call against a URI, then
//! streamed prose — but every number in it is real: real offsets, real line
//! ranges, real search hits.

use std::path::PathBuf;

use crate::book::{Book, Emphasis, Hits, Para};
use crate::i18n::{t, tf};
use crate::metrics;
use crate::ui::block::{ContextInfo, Event, PlanItem, Tool, Verb};
use crate::util::rand_range;

use super::turn::Step;

/// A position in the book: chapter index plus paragraph index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub chapter: usize,
    pub para: usize,
}

/// Characters of book text released per turn. Roughly one screen.
const READ_BUDGET: usize = 620;
/// Never split a turn below this many characters — avoids one-line turns.
const MIN_BUDGET: usize = 180;

/// Roll a position parked at the end of a chapter forward to the next one, so
/// the chapter counter never lags behind a "chapter complete" event.
pub fn normalize(book: &Book, mut pos: Pos) -> Pos {
    while let Some(chapter) = book.chapter(pos.chapter) {
        if pos.para < chapter.paras.len() {
            break;
        }
        pos = Pos {
            chapter: pos.chapter + 1,
            para: 0,
        };
    }
    pos
}

/// Compose the steps for "keep reading from `pos`".
///
/// The returned position is where reading will resume. It equals `pos` only
/// when the book is finished, which is also when the steps carry
/// [`Event::BookComplete`].
pub fn continue_reading(book: &Book, pos: Pos, resumed: bool) -> (Vec<Step>, Pos) {
    // A position parked at the end of a chapter rolls forward, so a keypress
    // never produces an empty turn.
    let pos = normalize(book, pos);
    let Some(chapter) = book.chapter(pos.chapter) else {
        return (vec![Step::Event(Event::BookComplete)], pos);
    };

    // 1. Decide how much to read.
    let mut end = pos.para;
    let mut chars = 0usize;
    while end < chapter.paras.len() {
        let para = &chapter.paras[end];
        let len = para.char_count();
        // A heading only leads a turn; never trail one.
        if end > pos.para && matches!(para, Para::Heading { .. }) && chars >= MIN_BUDGET {
            break;
        }
        chars += len;
        end += 1;
        if chars >= READ_BUDGET {
            break;
        }
    }
    let slice = &chapter.paras[pos.para..end];
    let next = Pos {
        chapter: pos.chapter,
        para: end,
    };
    let chapter_done = end >= chapter.paras.len();

    // 2. Reasoning, phrased against real numbers.
    let at_chapter_start = pos.para == 0;
    let thought = if resumed {
        tf(
            "flow.think_resumed",
            &[&(pos.chapter + 1), &chapter.title, &(pos.para + 1)],
        )
    } else if at_chapter_start {
        tf(
            "flow.think_chapter_start",
            &[
                &(pos.chapter + 1),
                &chapter.title,
                &chapter.paras.len(),
                &metrics::amount(chapter.char_count(), chapter.word_count()),
                &slice.len(),
            ],
        )
    } else {
        let tail = if chapter_done {
            t("flow.think_wraps_chapter").to_string()
        } else {
            tf("flow.think_paras_left", &[&(chapter.paras.len() - end)])
        };
        // A third of turns open with a filler word; the rest start flat. Every
        // turn sounding the same is what makes a fake agent feel fake.
        let filler = match rand_range(0, 3) {
            0 => t("flow.filler_ok"),
            1 => t("flow.filler_go_on"),
            _ => "",
        };
        tf(
            "flow.think_continue",
            &[&filler, &(pos.para + 1), &chars, &tail],
        )
    };

    // 3. The tool call that "fetches" the text.
    let line_start = book.line_of(pos.chapter, pos.para);
    let line_end = book.line_of(pos.chapter, end);
    let mut steps = vec![Step::Think(thought)];

    if resumed {
        steps.push(Step::Tool {
            tool: Tool::new(
                Verb::Locate,
                format!("{}/{}", book.uri_root(), chapter.href),
            )
            .detail(format!(
                "progress → ch{} p{}",
                pos.chapter + 1,
                pos.para + 1
            ))
            .body(vec![format!(
                "restored from ~/.readio/state.json  ·  {:.1}%",
                book.progress(pos.chapter, pos.para) * 100.0
            )]),
            ms: rand_range(140, 260),
        });
    }

    steps.push(Step::Tool {
        tool: Tool::new(Verb::Read, format!("{}/{}", book.uri_root(), chapter.href))
            .detail(format!("L{line_start}-{line_end}"))
            .body(preview(slice)),
        ms: rand_range(180, 460),
    });
    // Text and pictures alternate: a passage streams, an illustration appears
    // whole. Splitting here keeps the pacing logic ignorant of images.
    for run in runs(slice) {
        match run {
            Run::Text(paras) => {
                let (text, emphasis) = render_markdown(paras);
                steps.push(Step::Say { text, emphasis });
            }
            Run::Image { src, alt } => steps.push(Step::Image {
                path: src.clone(),
                alt: alt.clone(),
            }),
        }
    }

    if chapter_done {
        steps.push(Step::Event(Event::ChapterComplete {
            title: chapter.title.clone(),
            chars: chapter.char_count(),
        }));
        // Finishing a chapter is worth showing the progress write for real.
        steps.push(Step::Tool {
            tool: Tool::new(Verb::Write, "~/.readio/state.json")
                .detail(format!("ch{} complete", pos.chapter + 1))
                .body(vec![format!(
                    "{{ \"chapter\": {}, \"para\": {}, \"progress\": \"{:.1}%\" }}",
                    next.chapter,
                    next.para,
                    book.progress(next.chapter, next.para) * 100.0
                )]),
            ms: rand_range(90, 180),
        });
    }
    steps.push(Step::Advance {
        chapter: next.chapter,
        para: next.para,
    });
    (steps, next)
}

/// How many hits are listed. More than a dozen locations is a list nobody reads;
/// the true total is reported separately.
pub const HITS_SHOWN: usize = 12;

/// Steps for a free-form question: a real full-text search, presented as Grep.
///
/// Returns the steps and what was found, so the caller can keep the hits for
/// navigation instead of leaving the reader to copy locations by eye.
pub fn answer(book: &Book, question: &str) -> (Vec<Step>, String, Hits) {
    let asked = keyword(question);
    let (needle, hits) = resolve(book, &asked);

    let thought = if needle == asked {
        tf("flow.think_search", &[&needle])
    } else {
        tf("flow.think_narrow", &[&asked, &needle])
    };
    let root = book.uri_root();
    let body: Vec<String> = hits
        .shown
        .iter()
        .map(|hit| {
            let chapter = &book.chapters[hit.chapter];
            format!(
                "{}:{}  {}",
                short_href(&chapter.href),
                book.line_of(hit.chapter, hit.para),
                hit.excerpt
            )
        })
        .collect();

    // The tool line reports the true totals; `shown` is only what fits.
    let detail = if hits.truncated() {
        tf(
            "flow.grep_detail_capped",
            &[&needle, &hits.total, &hits.paragraphs, &hits.shown.len()],
        )
    } else {
        tf(
            "flow.grep_detail",
            &[&needle, &hits.total, &hits.paragraphs],
        )
    };
    let grep = Tool::new(Verb::Grep, root)
        .detail(detail)
        .body(if body.is_empty() {
            vec![t("flow.grep_none").to_string()]
        } else {
            body
        });
    let grep = if hits.is_empty() {
        grep.failed(t("flow.grep_zero"))
    } else {
        grep
    };

    let mut steps = vec![
        Step::Think(thought),
        Step::Tool {
            tool: grep,
            ms: rand_range(220, 520),
        },
    ];

    if hits.is_empty() {
        steps.push(Step::say(tf("flow.no_hits", &[&needle])));
        return (steps, needle, hits);
    }

    let mut answer = tf(
        "flow.hits_intro",
        &[&needle, &hits.total, &hits.paragraphs, &hits.shown.len()],
    );
    for (n, hit) in hits.shown.iter().enumerate() {
        let chapter = &book.chapters[hit.chapter];
        answer.push_str(&tf(
            "flow.hit_line",
            &[
                &format!("{:>2}", n + 1),
                &(hit.chapter + 1),
                &chapter.title,
                &(hit.para + 1),
                &hit.excerpt,
            ],
        ));
    }
    answer.push_str(t("flow.jump_hint"));
    steps.push(Step::say(answer));
    (steps, needle, hits)
}

/// Chapter listing, presented as a directory listing.
pub fn toc(book: &Book, current: usize) -> Vec<Step> {
    let body: Vec<String> = book
        .chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            format!(
                "{:>3}  {:<7}  {}{}",
                i + 1,
                metrics::amount(ch.char_count(), ch.word_count()),
                ch.title,
                if i == current {
                    t("flow.toc_current")
                } else {
                    ""
                }
            )
        })
        .collect();
    vec![
        Step::Think(tf(
            "flow.toc_intro",
            &[
                &book.chapters.len(),
                &metrics::amount(book.char_count(), book.word_count()),
            ],
        )),
        Step::Tool {
            tool: Tool::new(Verb::ListDir, book.uri_root())
                .detail(format!("{} entries", book.chapters.len()))
                .body(body),
            ms: rand_range(160, 320),
        },
    ]
}

/// Writing down a bookmark. The write is real, so it is shown as one.
pub fn marked(book: &Book, mark: &crate::store::Mark, count: usize) -> Vec<Step> {
    vec![
        Step::Tool {
            tool: Tool::new(Verb::Write, "~/.readio/state.json")
                .detail(format!("mark {count}"))
                .body(vec![format!(
                    "{{ \"chars\": {}, \"at\": \"{:.1}%\", \"label\": \"{}\" }}",
                    mark.chars,
                    book.progress(mark.chapter, mark.para) * 100.0,
                    mark.label
                )]),
            ms: rand_range(90, 180),
        },
        Step::Note(tf("cmd.mark_set", &[&count, &mark.label])),
    ]
}

/// The bookmarks of the open book, numbered for jumping to.
pub fn marks(book: &Book, marks: &[crate::store::Mark]) -> Vec<Step> {
    let body: Vec<String> = marks
        .iter()
        .enumerate()
        .map(|(i, mark)| {
            let (chapter, para) = book.locate(mark.chars as usize);
            format!(
                "{:>3}  ch{}:{}  {:>5.1}%  {}",
                i + 1,
                chapter + 1,
                para + 1,
                book.progress(chapter, para) * 100.0,
                mark.label
            )
        })
        .collect();
    vec![
        Step::Tool {
            tool: Tool::new(Verb::Read, "~/.readio/state.json")
                .detail(format!("{} marks", marks.len()))
                .body(body),
            ms: rand_range(120, 240),
        },
        Step::Note(t("cmd.marks_hint").to_string()),
    ]
}

/// Reading plan: the chapter list as a todo list.
/// How many chapters a plan shows at once. A to-do list is a glance, not an
/// index: a book cut at its table of contents can have seventy chapters, and
/// `/toc` is the command for the whole list.
const PLAN_WINDOW: usize = 9;

pub fn plan(book: &Book, pos: Pos) -> Vec<Step> {
    let total = book.chapters.len();
    // Keep the current chapter in view with a little of its past behind it.
    let start = pos
        .chapter
        .saturating_sub(2)
        .min(total.saturating_sub(PLAN_WINDOW));
    let end = (start + PLAN_WINDOW).min(total);
    let items = book.chapters[start..end]
        .iter()
        .enumerate()
        .map(|(offset, ch)| {
            let index = start + offset;
            PlanItem {
                number: index + 1,
                title: ch.title.clone(),
                chars: ch.char_count(),
                done: index < pos.chapter,
                current: index == pos.chapter,
            }
        })
        .collect();
    vec![Step::Plan {
        title: tf(
            "flow.plan_title",
            &[
                &book.title,
                &format!("{:.0}", book.progress(pos.chapter, pos.para) * 100.0),
            ],
        ),
        items,
        hidden: total - (end - start),
    }]
}

pub struct ContextArgs<'a> {
    pub session_chars: usize,
    pub cps: f32,
    pub elapsed: std::time::Duration,
    pub engine: Option<&'a str>,
}

pub fn context(book: &Book, pos: Pos, args: ContextArgs<'_>) -> Vec<Step> {
    let chapter = book
        .chapter(pos.chapter)
        .map(|c| c.title.clone())
        .unwrap_or_else(|| "—".to_string());
    vec![Step::Context(ContextInfo {
        book: match &book.author {
            Some(author) => format!("{} — {}", book.title, author),
            None => book.title.clone(),
        },
        chapter,
        chapter_index: pos.chapter.min(book.chapters.len().saturating_sub(1)),
        chapter_total: book.chapters.len(),
        progress: book.progress(pos.chapter, pos.para),
        chars_read: book.chars_before(pos.chapter, pos.para),
        chars_total: book.char_count(),
        chapter_chars: book
            .chapter(pos.chapter)
            .map(|c| c.char_count())
            .unwrap_or(0),
        session_chars: args.session_chars,
        cps: args.cps,
        elapsed: args.elapsed,
        engine: args.engine.map(|e| e.to_string()),
    })]
}

/// Opening card for a freshly loaded book.
pub fn welcome(book: &Book, pos: Pos, resumed: bool) -> Vec<Step> {
    let head = if resumed {
        tf(
            "flow.welcome_resume",
            &[
                &book.title,
                &format!("{:.1}", book.progress(pos.chapter, pos.para) * 100.0),
                &(pos.chapter + 1),
            ],
        )
    } else {
        tf(
            "flow.welcome_fresh",
            &[
                &book.title,
                &book.chapters.len(),
                &metrics::amount(book.char_count(), book.word_count()),
            ],
        )
    };
    let mut steps = vec![Step::Note(head)];
    // The cover belongs to opening a book, not to going back to one: a picture
    // that reappears every time the reader presses enter on their history is
    // furniture, not a cover.
    if !resumed && let Some(cover) = &book.cover {
        let bytes = std::fs::metadata(&cover.file).map(|m| m.len()).unwrap_or(0);
        steps.push(Step::Tool {
            tool: Tool::new(Verb::Read, format!("{}/{}", book.uri_root(), cover.href))
                .detail(t("flow.cover").to_string())
                .body(vec![format!("{}  ·  {}", cover.href, size(bytes))]),
            ms: rand_range(120, 240),
        });
        steps.push(Step::Image {
            path: cover.file.clone(),
            // The tool line above already said this is the cover; the caption is
            // better spent naming the book it belongs to.
            alt: book.title.clone(),
        });
    }
    steps
}

/// Bytes as a reader would write them.
fn size(bytes: u64) -> String {
    match bytes {
        0 => "—".to_string(),
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.0} KB", n as f64 / 1024.0),
        n => format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// One stretch of a turn: either text to stream or a picture to show.
enum Run<'a> {
    Text(&'a [Para]),
    Image { src: PathBuf, alt: String },
}

/// Split paragraphs at every illustration, keeping the order.
fn runs(paras: &[Para]) -> Vec<Run<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (index, para) in paras.iter().enumerate() {
        if let Para::Image { src, alt } = para {
            if index > start {
                out.push(Run::Text(&paras[start..index]));
            }
            out.push(Run::Image {
                src: src.clone(),
                alt: alt.clone(),
            });
            start = index + 1;
        }
    }
    if start < paras.len() {
        out.push(Run::Text(&paras[start..]));
    }
    out
}

/// Paragraphs → the passage dialect understood by the passage renderer, plus
/// the emphasis found in them, moved into the passage's own coordinates.
fn render_markdown(paras: &[Para]) -> (String, Vec<Emphasis>) {
    let mut out = String::new();
    let mut emphasis: Vec<Emphasis> = Vec::new();
    for (i, para) in paras.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        // Where this paragraph's own text starts in the passage, once whatever
        // marker precedes it has been written.
        let mut carry = |out: &mut String, marker: &str| {
            out.push_str(marker);
            let base = out.len() as u32;
            for span in para.emphasis() {
                emphasis.push(Emphasis {
                    start: base + span.start,
                    end: base + span.end,
                    strong: span.strong,
                });
            }
        };
        match para {
            Para::Heading { text, .. } => {
                out.push_str("## ");
                out.push_str(text);
            }
            Para::Text(rich) => {
                carry(&mut out, "");
                out.push_str(&rich.text);
            }
            Para::Quote(rich) => {
                carry(&mut out, "> ");
                out.push_str(&rich.text);
            }
            Para::Code(text) => {
                out.push_str("```\n");
                out.push_str(text);
                out.push_str("\n```");
            }
            // Pictures become their own step; a stray one reads as its caption.
            Para::Image { alt, .. } => {
                if alt.trim().is_empty() {
                    out.push_str(t("flow.figure_inline"));
                } else {
                    out.push_str("> ");
                    out.push_str(alt);
                }
            }
        }
    }
    (out, emphasis)
}

/// Dim preview lines under a Read call: enough to prove it read something.
fn preview(paras: &[Para]) -> Vec<String> {
    paras
        .iter()
        .take(3)
        .map(|para| match para {
            Para::Heading { level, text } => format!("{} {}", "#".repeat(*level as usize), text),
            other => trim_to(other.text(), 76),
        })
        .collect()
}

fn trim_to(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, c) in text.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

fn short_href(href: &str) -> String {
    href.rsplit('/').next().unwrap_or(href).to_string()
}

/// Reduce a question to something worth grepping for.
/// Find a needle that actually matches, narrowing the question if it does not.
///
/// Chinese has no spaces, so a question arrives as one unbroken string and
/// "忽必烈帝国是什么样子" matches nothing — while 忽必烈 is on every other page.
/// Try what was asked, then progressively shorter pieces of it, and report which
/// one was used rather than pretending the book is silent.
fn resolve(book: &Book, asked: &str) -> (String, Hits) {
    let hits = book.search(asked, HITS_SHOWN);
    if !hits.is_empty() || asked.is_empty() {
        return (asked.to_string(), hits);
    }
    for candidate in candidates(asked) {
        let hits = book.search(&candidate, HITS_SHOWN);
        if !hits.is_empty() {
            return (candidate, hits);
        }
    }
    (asked.to_string(), Hits::default())
}

/// Shorter needles to try, best first.
///
/// For a spaced language the words themselves are the candidates, longest
/// first. For Chinese, drop the interrogative tail — a question ends with what
/// it is asking, and begins with what it is asking about — then slide a window
/// down from the whole phrase to two characters.
fn candidates(asked: &str) -> Vec<String> {
    if asked.split_whitespace().count() > 1 {
        let mut words: Vec<String> = asked
            .split_whitespace()
            .filter(|word| word.chars().count() > 2)
            .map(str::to_string)
            .collect();
        words.sort_by_key(|word| std::cmp::Reverse(word.chars().count()));
        return words;
    }

    const TAILS: &[&str] = &[
        "是什么样子",
        "什么样子",
        "是什么",
        "什么样",
        "怎么样",
        "为什么",
        "什么",
        "怎么",
        "如何",
        "多少",
        "哪些",
        "哪个",
        "样子",
        "是",
        "有",
    ];
    let mut trimmed = asked.to_string();
    loop {
        let before = trimmed.chars().count();
        for tail in TAILS {
            if let Some(rest) = trimmed.strip_suffix(tail)
                && rest.chars().count() >= 2
            {
                trimmed = rest.to_string();
                break;
            }
        }
        if trimmed.chars().count() == before {
            break;
        }
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut out = Vec::new();
    if trimmed != asked && chars.len() >= 2 {
        out.push(trimmed.clone());
    }
    for width in (2..chars.len()).rev() {
        for start in 0..=chars.len() - width {
            let piece: String = chars[start..start + width].iter().collect();
            if !out.contains(&piece) {
                out.push(piece);
            }
        }
    }
    out
}

fn keyword(question: &str) -> String {
    let stripped: String = question
        .chars()
        .filter(|c| !"?？!!。,,、;;::「」“”‘’《》()()【】的了吗呢吧啊".contains(*c))
        .collect();
    let cleaned = stripped.trim();
    if cleaned.chars().count() <= 12 && !cleaned.is_empty() {
        return cleaned.to_string();
    }
    // Long question: keep the longest whitespace-free run, which for Chinese is
    // usually the noun phrase being asked about.
    cleaned
        .split_whitespace()
        .max_by_key(|t| t.chars().count())
        .map(|t| trim_to(t, 12))
        .unwrap_or_else(|| trim_to(cleaned, 12))
}

#[cfg(test)]
mod narrowing_tests {
    use super::*;

    #[test]
    fn a_chinese_question_offers_its_topic_before_its_fragments() {
        let list = candidates("忽必烈帝国是什么样子");
        assert_eq!(
            list.first().map(String::as_str),
            Some("忽必烈帝国"),
            "the interrogative tail goes first: {list:?}"
        );
        let kublai = list.iter().position(|c| c == "忽必烈");
        let noise = list.iter().position(|c| c == "国是");
        assert!(kublai.is_some(), "忽必烈 must be tried: {list:?}");
        assert!(
            kublai < noise || noise.is_none(),
            "a longer prefix should come before a two-character fragment: {list:?}"
        );
    }

    #[test]
    fn a_spaced_question_falls_back_to_its_longest_words() {
        let list = candidates("what shape does attention have");
        assert_eq!(list.first().map(String::as_str), Some("attention"));
        assert!(
            !list.iter().any(|word| word.chars().count() <= 2),
            "two-letter words are noise: {list:?}"
        );
    }

    #[test]
    fn a_phrase_that_needs_no_narrowing_produces_nothing_to_try() {
        // Two characters cannot be narrowed further.
        assert!(candidates("记忆").is_empty());
    }
}
