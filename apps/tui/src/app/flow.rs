//! Reading logic: turning book positions and user intent into turn steps.
//!
//! This is where the disguise is decided. A reading action is expressed the way
//! a coding agent would express it — a thought, a tool call against a URI, then
//! streamed prose — but every number in it is real: real offsets, real line
//! ranges, real search hits.

use std::path::PathBuf;

use crate::book::{Book, Para};
use crate::i18n::{t, tf};
use crate::ui::block::{ContextInfo, Event, PlanItem, Tool, Verb};
use crate::util::{human, rand_range};

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
                &human(chapter.char_count()),
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
            Run::Text(paras) => steps.push(Step::Say(render_markdown(paras))),
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

/// Steps for a free-form question: a real full-text search, presented as Grep.
pub fn answer(book: &Book, question: &str) -> Vec<Step> {
    let needle = keyword(question);
    let hits = book.search(&needle, 12);

    let thought = tf("flow.think_search", &[&needle]);
    let root = book.uri_root();
    let body: Vec<String> = hits
        .iter()
        .map(|(ci, pi, excerpt)| {
            let chapter = &book.chapters[*ci];
            format!(
                "{}:{}  {}",
                short_href(&chapter.href),
                book.line_of(*ci, *pi),
                excerpt
            )
        })
        .collect();

    let grep = Tool::new(Verb::Grep, root)
        .detail(format!("pattern: {needle}  ·  {} hits", hits.len()))
        .body(if body.is_empty() {
            vec!["no matches".to_string()]
        } else {
            body
        });
    let grep = if hits.is_empty() {
        grep.failed("0 matches")
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
        steps.push(Step::Say(tf("flow.no_hits", &[&needle])));
        return steps;
    }

    let mut answer = tf("flow.hits_intro", &[&needle, &hits.len()]);
    for (ci, pi, _) in hits.iter().take(3) {
        let chapter = &book.chapters[*ci];
        let para = &chapter.paras[*pi];
        answer.push('\n');
        answer.push_str(&format!("> {}\n", trim_to(para.text(), 220)));
        answer.push_str(&tf(
            "flow.hit_location",
            &[&(ci + 1), &chapter.title, &(pi + 1)],
        ));
    }
    answer.push_str(t("flow.where_to_start"));
    if let Some((ci, pi, _)) = hits.first() {
        answer.push_str(&tf("flow.goto_hint", &[&(ci + 1)]));
        answer.push_str(&tf("flow.goto_para", &[&(pi + 1)]));
    }
    steps.push(Step::Say(answer));
    steps
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
                tf("block.chars", &[&human(ch.char_count())]),
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
            &[&book.chapters.len(), &human(book.char_count())],
        )),
        Step::Tool {
            tool: Tool::new(Verb::ListDir, book.uri_root())
                .detail(format!("{} entries", book.chapters.len()))
                .body(body),
            ms: rand_range(160, 320),
        },
    ]
}

/// Reading plan: the chapter list as a todo list.
pub fn plan(book: &Book, pos: Pos) -> Vec<Step> {
    let items = book
        .chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| PlanItem {
            title: ch.title.clone(),
            chars: ch.char_count(),
            done: i < pos.chapter,
            current: i == pos.chapter,
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
            &[&book.title, &book.chapters.len(), &human(book.char_count())],
        )
    };
    vec![Step::Note(head)]
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

/// Paragraphs → the passage dialect understood by the passage renderer.
fn render_markdown(paras: &[Para]) -> String {
    let mut out = String::new();
    for (i, para) in paras.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        match para {
            Para::Heading { text, .. } => {
                out.push_str("## ");
                out.push_str(text);
            }
            Para::Text(text) => out.push_str(text),
            Para::Quote(text) => {
                out.push_str("> ");
                out.push_str(text);
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
    out
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
