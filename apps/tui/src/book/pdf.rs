//! PDF loading, restricted to files that actually carry text.
//!
//! A PDF describes glyph placements, not prose: every typeset line arrives as
//! its own line, running headers and page numbers sit in the same stream as the
//! body, and a scanned book carries no text at all. This module rebuilds the
//! shape a reader needs — paragraphs, chapters, page provenance — and refuses
//! the files where guessing would not help.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use pdf_extract::{Dictionary, Document, Object, ObjectId};

use super::{Book, Chapter, Para, Source};

/// Mean characters per page below which a PDF is judged a scan, not a text.
const SCAN_CHARS_PER_PAGE: usize = 25;

/// Size the page-grouping fallback aims for when there are no headings to go on.
const GROUP_CHARS: usize = 3000;

/// Longest line still allowed to pass for a heading.
const HEADING_MAX_CHARS: usize = 32;

/// Load a text-based PDF, or explain why this one cannot be read.
pub fn load(path: &Path) -> Result<Book> {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book.pdf".to_string());

    let doc = open(path)?;
    let pages = page_texts(&doc);
    anyhow::ensure!(
        !pages.is_empty(),
        "{} 里一页都没有，可能不是完整的 PDF",
        file
    );

    let letters: usize = pages
        .iter()
        .map(|p| p.chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    let per_page = letters / pages.len();
    if per_page < SCAN_CHARS_PER_PAGE {
        return Err(anyhow!(
            "这份 PDF 看起来是扫描件（每页只有 {per_page} 个字符），readio 只能读文字型 PDF"
        ));
    }

    let lines = strip_boilerplate(&split_lines(&pages));
    let blocks = blocks(&lines);
    anyhow::ensure!(
        !blocks.is_empty(),
        "{} 的文字全被当成页眉页脚滤掉了，readio 读不出正文",
        file
    );

    let groups = outline_groups(&doc, &blocks)
        .or_else(|| heading_groups(&blocks))
        .unwrap_or_else(|| page_groups(&blocks));
    let chapters = assemble(&groups, &file);
    anyhow::ensure!(!chapters.is_empty(), "{} 里没有可读的章节", file);

    let title = info_text(&doc, b"Title").unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    });

    Ok(Book {
        id: String::new(),
        title,
        author: info_text(&doc, b"Author"),
        path: Some(path.to_path_buf()),
        source: Source::Pdf,
        chapters,
    })
}

// ── extraction ───────────────────────────────────────────────────────────────

/// Open the document, decrypting the "no password, just permissions" case that
/// most publisher PDFs use.
fn open(path: &Path) -> Result<Document> {
    let quiet = QuietPanics::install();
    let loaded = std::panic::catch_unwind(|| Document::load(path));
    drop(quiet);

    let mut doc = match loaded {
        Ok(result) => {
            result.with_context(|| crate::i18n::tf("pdf.unreadable", &[&path.display()]))?
        }
        Err(_) => {
            return Err(anyhow!(
                "{}",
                crate::i18n::tf("pdf.broken", &[&path.display()])
            ));
        }
    };
    if doc.is_encrypted() {
        doc.decrypt("")
            .map_err(|_| anyhow!("{}", crate::i18n::t("pdf.encrypted")))?;
    }
    Ok(doc)
}

/// Plain text of every page, in reading order.
fn page_texts(doc: &Document) -> Vec<String> {
    let quiet = QuietPanics::install();
    let pages: Vec<String> = doc
        .get_pages()
        .keys()
        .copied()
        .map(|number| page_text(doc, number))
        .collect();
    drop(quiet);
    pages
}

/// One page's text, or nothing if the extractor gives up on it.
///
/// pdf-extract panics on pages that lack a media box or whose content stream it
/// cannot decode. Losing that page is a fair price; losing the book is not.
fn page_text(doc: &Document, number: u32) -> String {
    let extracted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut text = String::new();
        {
            let mut sink = pdf_extract::PlainTextOutput::new(&mut text);
            if pdf_extract::output_doc_page(doc, &mut sink, number).is_err() {
                return String::new();
            }
        }
        text
    }));
    extracted.unwrap_or_default()
}

/// The hook `std::panic::take_hook` hands back.
type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// Suppresses the default panic hook for as long as it is alive.
///
/// The panics raised inside pdf-extract are caught, but the hook would still
/// print them — straight over a terminal UI that is mid-frame.
struct QuietPanics(Option<PanicHook>);

impl QuietPanics {
    fn install() -> Self {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        Self(Some(previous))
    }
}

impl Drop for QuietPanics {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            std::panic::set_hook(previous);
        }
    }
}

// ── page furniture ───────────────────────────────────────────────────────────

/// Split every page into trimmed, non-empty lines.
fn split_lines(pages: &[String]) -> Vec<Vec<String>> {
    pages
        .iter()
        .map(|page| {
            page.lines()
                .map(collapse_ws)
                .filter(|line| !line.is_empty())
                .collect()
        })
        .collect()
}

/// Remove running headers, footers and page numbers.
///
/// The "repeats on most pages" rule is applied only to the outermost lines of a
/// page: a sentence that recurs in the middle of the prose is the author's
/// doing, and deleting it would silently eat content.
fn strip_boilerplate(pages: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for lines in pages {
        let mut once = HashSet::new();
        for index in edge_indices(lines.len()) {
            let key = fingerprint(&lines[index]);
            if once.insert(key.clone()) {
                *seen.entry(key).or_default() += 1;
            }
        }
    }
    // Three pages is the least that can show a habit rather than a coincidence.
    let repeated = |line: &str| {
        pages.len() >= 3
            && seen.get(&fingerprint(line)).copied().unwrap_or(0) * 10 >= pages.len() * 6
    };

    pages
        .iter()
        .map(|lines| {
            let edges = edge_indices(lines.len());
            lines
                .iter()
                .enumerate()
                .filter(|(index, line)| {
                    if edges.contains(index) {
                        return !is_page_number(line) && !repeated(line);
                    }
                    !line.chars().all(|c| c.is_ascii_digit())
                })
                .map(|(_, line)| line.clone())
                .collect()
        })
        .collect()
}

/// Positions a header or footer can occupy: the first and last two lines.
fn edge_indices(len: usize) -> Vec<usize> {
    (0..len)
        .filter(|index| *index < 2 || index + 2 >= len)
        .collect()
}

/// Comparison key that lets `Page 3` and `Page 4` count as the same header.
fn fingerprint(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_number = false;
    for c in line.chars().filter(|c| !c.is_whitespace()) {
        if c.is_ascii_digit() {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// A line that is nothing but a page number: `12`, `— 12 —`, `第 12 页`,
/// `Page 3 of 9`, `iv`.
fn is_page_number(line: &str) -> bool {
    let bare = line
        .trim()
        .trim_matches(|c: char| c.is_whitespace() || "-–—·.[]()<>|/*".contains(c))
        .trim();
    if bare.is_empty() {
        return false;
    }
    if bare.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if bare.len() <= 6 && bare.chars().all(|c| "ivxlcdmIVXLCDM".contains(c)) {
        return true;
    }
    let digits_only = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if let Some(rest) = bare.strip_prefix('第')
        && let Some(number) = rest.strip_suffix('页')
    {
        return digits_only(number.trim());
    }
    let lower = bare.to_lowercase();
    let body = lower.strip_prefix("page").unwrap_or(&lower).trim();
    let words: Vec<&str> = body
        .split(|c: char| c.is_whitespace() || c == '/')
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [n] => digits_only(n),
        // `3 of 9` and `3 / 9` are footers; two bare numbers side by side are
        // just as likely to be a pair of years in the prose.
        [n, "of", m] => digits_only(n) && digits_only(m),
        [n, m] => {
            digits_only(n) && digits_only(m) && (lower.starts_with("page") || bare.contains('/'))
        }
        _ => false,
    }
}

// ── paragraphs ───────────────────────────────────────────────────────────────

/// A reconstructed paragraph, tagged with the pages it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    page: usize,
    /// Last page the text reaches; larger than `page` for a paragraph that was
    /// interrupted by a page break.
    through: usize,
    text: String,
    heading: bool,
}

/// Rebuild paragraphs from the hard line breaks that make up each page.
fn blocks(pages: &[Vec<String>]) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    // Whether the previous page stopped mid-paragraph.
    let mut carry = false;

    for (index, lines) in pages.iter().enumerate() {
        let page = index + 1;
        for (order, text) in paragraphs(lines).into_iter().enumerate() {
            let heading = is_heading_like(&text);
            let continues = carry
                && order == 0
                && !heading
                && !starts_new_block(&text)
                && out.last().is_some_and(|prev| !prev.heading);
            match out.last_mut() {
                Some(prev) if continues => {
                    prev.text = join_lines(&prev.text, &text);
                    prev.through = page;
                }
                _ => out.push(Block {
                    page,
                    through: page,
                    text,
                    heading,
                }),
            }
        }
        carry = ends_mid_paragraph(lines);
    }
    out
}

/// Join one page's lines into paragraphs.
///
/// The lines of a paragraph are glued back together unless the previous line
/// closed a sentence, was too short to be anything but a paragraph's last line,
/// or the next line opens a block of its own. Over-splitting only yields short
/// paragraphs; under-splitting welds a heading onto the prose above it.
fn paragraphs(lines: &[String]) -> Vec<String> {
    let typical = typical_len(lines);
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in lines {
        if !current.is_empty() && starts_new_block(line) {
            out.push(std::mem::take(&mut current));
        }
        if current.is_empty() {
            current.push_str(line);
        } else {
            current = join_lines(&current, line);
        }
        if ends_sentence(line) || is_short(line, typical) || is_heading_like(line) {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out.retain(|para| !para.trim().is_empty());
    out
}

/// Whether a page's last line leaves a paragraph hanging into the next page.
fn ends_mid_paragraph(lines: &[String]) -> bool {
    let typical = typical_len(lines);
    lines.last().is_some_and(|last| {
        !ends_sentence(last) && !is_short(last, typical) && !is_heading_like(last)
    })
}

/// Median line length on a page, or zero when there are too few lines for the
/// median to mean anything.
fn typical_len(lines: &[String]) -> usize {
    if lines.len() < 3 {
        return 0;
    }
    let mut lengths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
    lengths.sort_unstable();
    lengths[lengths.len() / 2]
}

/// A line noticeably narrower than the page's measure, i.e. a paragraph's last.
fn is_short(line: &str, typical: usize) -> bool {
    typical >= 24 && line.chars().count() * 4 < typical * 3
}

/// Glue two lines of the same paragraph together.
///
/// English hyphenation is undone, CJK lines are joined flush — a space between
/// two Han characters would read as a gap the typesetter never intended.
fn join_lines(prev: &str, next: &str) -> String {
    let head = prev.trim_end();
    let tail = next.trim_start();
    if tail.is_empty() {
        return head.to_string();
    }
    let last = head.chars().next_back();
    let before_last = head.chars().rev().nth(1);
    let first = tail.chars().next();

    // `exam-` + `ple`: a hyphen between two lowercase Latin fragments was added
    // by the typesetter. `Sino-` + `American` is a real compound, so it keeps
    // the hyphen — but neither case wants a space.
    if last == Some('-') && before_last.is_some_and(|c| c.is_ascii_alphabetic()) {
        let stem = match first {
            Some(c) if c.is_ascii_lowercase() => &head[..head.len() - 1],
            _ => head,
        };
        return format!("{stem}{tail}");
    }

    let mut joined = head.to_string();
    let flush = last.is_some_and(is_wide) && first.is_some_and(is_wide);
    if !flush {
        joined.push(' ');
    }
    joined.push_str(tail);
    joined
}

/// Characters that carry their own spacing: CJK, kana, full-width punctuation.
fn is_wide(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFF60)
}

/// Whether a line closes a sentence, ignoring the quotes and brackets that can
/// trail the punctuation.
fn ends_sentence(line: &str) -> bool {
    let core = line
        .trim_end()
        .trim_end_matches(['”', '’', '"', '\'', '）', ')', '』', '」', '》', '〉', ']']);
    core.ends_with(['。', '！', '？', '…', '.', '!', '?'])
}

/// Whether a line opens a block of its own: a bullet, a list number, a heading.
fn starts_new_block(line: &str) -> bool {
    let chars: Vec<char> = line.trim_start().chars().collect();
    let Some(first) = chars.first().copied() else {
        return false;
    };
    if "•·●○▪◦‣※*".contains(first) {
        return true;
    }
    if "-–—".contains(first) && chars.get(1).is_some_and(|c| c.is_whitespace()) {
        return true;
    }
    if first == '(' || first == '（' {
        let inner: String = chars[1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !inner.is_empty()
            && chars
                .get(1 + inner.chars().count())
                .is_some_and(|c| *c == ')' || *c == '）')
        {
            return true;
        }
    }
    let label: String = chars.iter().take_while(|c| !c.is_whitespace()).collect();
    let marker = |set: &str, tail: &str| {
        let digits: String = label.chars().take_while(|c| set.contains(*c)).collect();
        !digits.is_empty()
            && label
                .chars()
                .nth(digits.chars().count())
                .is_some_and(|c| tail.contains(c))
            && label.chars().count() <= digits.chars().count() + 1
    };
    if marker("0123456789", ".)、．") || marker("零〇一二三四五六七八九十", "、.．")
    {
        return true;
    }
    is_heading_like(line)
}

/// Whether a line reads like a heading: short, unpunctuated, and shaped like a
/// chapter label.
///
/// Strict on purpose. Mistaking a line of prose for a heading invents a chapter
/// boundary in the middle of a sentence, while missing one merely leaves two
/// sections joined — the cheaper failure, as in the plain-text loader.
fn is_heading_like(line: &str) -> bool {
    let line = line.trim();
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() || chars.len() > HEADING_MAX_CHARS {
        return false;
    }
    if line.ends_with([
        '。', '，', '、', '；', '：', '！', '？', '…', ',', '.', ';', ':', '!', '?',
    ]) {
        return false;
    }
    chapter_label(line)
}

/// The handful of shapes a chapter title actually takes.
fn chapter_label(line: &str) -> bool {
    let lower = line.to_lowercase();
    if ["chapter ", "part ", "section ", "appendix ", "book "]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    if [
        "contents",
        "preface",
        "foreword",
        "introduction",
        "epilogue",
        "index",
        "bibliography",
        "前言",
        "序",
        "序言",
        "目录",
        "引言",
        "后记",
        "结语",
        "附录",
        "致谢",
        "参考文献",
    ]
    .contains(&lower.as_str())
    {
        return true;
    }
    // `3.1 方法`: a multi-part number followed by a title. A lone `1.` is left
    // to the list-marker rule, which opens a block but not a chapter.
    let (number, rest) = lower
        .split_once(char::is_whitespace)
        .unwrap_or((&lower, ""));
    let parts: Vec<&str> = number.trim_end_matches('.').split('.').collect();
    if !rest.trim().is_empty()
        && parts.len() >= 2
        && parts.iter().all(|part| {
            part.len() <= 3 && !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return true;
    }
    chinese_chapter_label(line)
}

/// `第三章`, `第 12 节 命名`: 第 + a numeral + a unit, with only a separator or
/// a title behind it.
fn chinese_chapter_label(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.first() != Some(&'第') {
        return false;
    }
    let Some(unit) = chars.iter().position(|c| "章节卷回篇部".contains(*c)) else {
        return false;
    };
    let numeral = "0123456789零〇一二三四五六七八九十百千两 ";
    let number_ok = unit > 1 && chars[1..unit].iter().all(|c| numeral.contains(*c));
    let tail_ok = chars
        .get(unit + 1)
        .is_none_or(|c| c.is_whitespace() || "：:·-—、.".contains(*c));
    number_ok && tail_ok
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

// ── chapters ─────────────────────────────────────────────────────────────────

/// A chapter before it has a title or an href: an optional name and its blocks.
type Group<'a> = (Option<String>, Vec<&'a Block>);

/// Turn groups into chapters, naming the unnamed ones after their page range.
fn assemble(groups: &[Group<'_>], file: &str) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    for (title, blocks) in groups.iter().filter(|(_, blocks)| !blocks.is_empty()) {
        let span = page_span(blocks);
        let title = title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| crate::i18n::tf("pdf.part", &[&(chapters.len() + 1), &span]));
        chapters.push(Chapter {
            title,
            href: format!("{file}#{span}"),
            paras: blocks
                .iter()
                .map(|block| {
                    if block.heading {
                        Para::Heading {
                            level: 2,
                            text: block.text.clone(),
                        }
                    } else {
                        Para::Text(block.text.clone())
                    }
                })
                .collect(),
        });
    }
    chapters
}

/// `p5-9` for a chapter that spans pages, `p5` for one that does not.
fn page_span(blocks: &[&Block]) -> String {
    let first = blocks.first().map(|b| b.page).unwrap_or(1);
    let last = blocks.iter().map(|b| b.through).max().unwrap_or(first);
    if first == last {
        format!("p{first}")
    } else {
        format!("p{first}-{last}")
    }
}

/// Chapters from the PDF outline, when its bookmarks line up with the text.
fn outline_groups<'a>(doc: &Document, blocks: &'a [Block]) -> Option<Vec<Group<'a>>> {
    let marks = outline(doc);
    if marks.len() < 2 {
        return None;
    }
    let mut groups = Vec::new();
    for (index, (page, title)) in marks.iter().enumerate() {
        let end = marks.get(index + 1).map(|(p, _)| *p).unwrap_or(usize::MAX);
        // Front matter ahead of the first bookmark rides along with chapter one
        // rather than becoming a chapter nobody named.
        let start = if index == 0 { 0 } else { *page };
        let slice: Vec<&Block> = blocks
            .iter()
            .filter(|block| block.page >= start && block.page < end)
            .collect();
        if slice.is_empty() {
            return None;
        }
        groups.push((Some(title.clone()), slice));
    }
    Some(groups)
}

/// Top-level bookmarks as (page, title).
///
/// Only destinations that name a page outright are followed. Named
/// destinations live in a name tree whose traversal costs far more than a
/// chapter title is worth, and the heading fallback covers those files.
fn outline(doc: &Document) -> Vec<(usize, String)> {
    let numbers: HashMap<ObjectId, usize> = doc
        .get_pages()
        .into_iter()
        .map(|(number, id)| (id, number as usize))
        .collect();
    let Ok(catalog) = doc.catalog() else {
        return Vec::new();
    };
    let Some(root) = catalog.get(b"Outlines").ok().and_then(|o| as_dict(doc, o)) else {
        return Vec::new();
    };

    let mut marks: Vec<(usize, String)> = Vec::new();
    let mut next = root.get(b"First").ok().and_then(|o| o.as_reference().ok());
    let mut visited = HashSet::new();
    while let Some(id) = next {
        if !visited.insert(id) || visited.len() > 512 {
            break;
        }
        let Ok(item) = doc.get_dictionary(id) else {
            break;
        };
        if let (Some(title), Some(page)) = (
            item.get(b"Title")
                .ok()
                .and_then(|o| as_text(doc, o))
                .filter(|t| t.chars().count() <= 120),
            dest_page(doc, item, &numbers),
        ) {
            marks.push((page, title));
        }
        next = item.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }
    marks.sort_by_key(|(page, _)| *page);
    marks.dedup_by_key(|(page, _)| *page);
    marks
}

/// Page a bookmark points at, via either `/Dest` or a `/GoTo` action.
fn dest_page(
    doc: &Document,
    item: &Dictionary,
    numbers: &HashMap<ObjectId, usize>,
) -> Option<usize> {
    let direct = item.get(b"Dest").ok();
    let via_action = item
        .get(b"A")
        .ok()
        .and_then(|action| as_dict(doc, action))
        .and_then(|action| action.get(b"D").ok());
    let dest = direct.or(via_action)?;
    let dest = doc.dereference(dest).map(|(_, o)| o).unwrap_or(dest);
    let target = dest.as_array().ok()?.first()?;
    if let Ok(id) = target.as_reference() {
        return numbers.get(&id).copied();
    }
    // A zero-based page index, the form named destinations use.
    target
        .as_i64()
        .ok()
        .and_then(|index| usize::try_from(index).ok())
        .map(|index| index + 1)
}

/// Chapters from heading-like lines, when they cut the book up sensibly.
fn heading_groups(blocks: &[Block]) -> Option<Vec<Group<'_>>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    for block in blocks {
        let closed = groups
            .last()
            .is_none_or(|(_, blocks)| blocks.iter().any(|b| !b.heading));
        if block.heading && closed {
            groups.push((Some(block.text.clone()), Vec::new()));
        }
        match groups.last_mut() {
            Some((_, current)) => current.push(block),
            None => groups.push((None, vec![block])),
        }
    }
    // A table of contents is a run of heading-like lines with no prose between
    // them; splitting there would produce a shelf of empty chapters.
    let usable = groups.len() >= 2
        && groups
            .iter()
            .all(|(_, blocks)| blocks.iter().any(|b| !b.heading));
    usable.then_some(groups)
}

/// Last resort: page-aligned chunks of roughly `GROUP_CHARS` characters.
fn page_groups(blocks: &[Block]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut current: Vec<&Block> = Vec::new();
    let mut chars = 0usize;

    for (index, block) in blocks.iter().enumerate() {
        current.push(block);
        chars += block.text.chars().count();
        let page_turns = blocks
            .get(index + 1)
            .is_some_and(|next| next.page > block.page);
        if chars >= GROUP_CHARS && page_turns {
            groups.push((None, std::mem::take(&mut current)));
            chars = 0;
        }
    }
    if !current.is_empty() {
        // A stub left at the end reads better appended than as its own chapter.
        match groups.last_mut() {
            Some((_, last)) if chars < GROUP_CHARS / 3 => last.append(&mut current),
            _ => groups.push((None, current)),
        }
    }
    groups
}

// ── document metadata ────────────────────────────────────────────────────────

/// A string from the document information dictionary, if it has one worth using.
fn info_text(doc: &Document, key: &[u8]) -> Option<String> {
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| as_dict(doc, object))?;
    info.get(key)
        .ok()
        .and_then(|object| as_text(doc, object))
        .filter(|text| !text.is_empty() && text.chars().count() <= 120)
}

fn as_dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(object)
        .map(|(_, resolved)| resolved)
        .unwrap_or(object)
        .as_dict()
        .ok()
}

/// Decode a PDF text string: UTF-16BE when it carries a byte-order mark,
/// PDFDocEncoding (Latin-1 for everything we care about) otherwise.
fn as_text(doc: &Document, object: &Object) -> Option<String> {
    let bytes = doc
        .dereference(object)
        .map(|(_, resolved)| resolved)
        .unwrap_or(object)
        .as_str()
        .ok()?;
    let decoded = if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|b| *b as char).collect()
    };
    let cleaned = collapse_ws(&decoded);
    (!cleaned.is_empty()).then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    fn text_block(page: usize, text: &str) -> Block {
        Block {
            page,
            through: page,
            text: text.to_string(),
            heading: false,
        }
    }

    fn heading_block(page: usize, text: &str) -> Block {
        Block {
            heading: true,
            ..text_block(page, text)
        }
    }

    #[test]
    fn wrapped_lines_become_one_paragraph() {
        let lines = page(&[
            "The quick brown fox jumped over the lazy dog and kept",
            "running down the road until it reached the river bank,",
            "where it finally stopped to drink.",
        ]);
        let paras = paragraphs(&lines);
        assert_eq!(
            paras.len(),
            1,
            "three wrapped lines are one paragraph, got {paras:?}"
        );
        assert!(
            paras[0].contains("kept running down"),
            "the line break should become a single space: {}",
            paras[0]
        );
        assert!(
            !paras[0].contains('\n'),
            "no hard break may survive: {}",
            paras[0]
        );
    }

    #[test]
    fn a_closed_sentence_ends_the_paragraph() {
        let lines = page(&[
            "他把书合上，站起来走到窗前，外面正在下雨。",
            "第二天早上，雨停了，他决定继续读下去，一直读到天黑",
            "为止，屋子里只剩下翻页的声音。",
        ]);
        let paras = paragraphs(&lines);
        assert_eq!(
            paras.len(),
            2,
            "the full stop at the end of line one should close it, got {paras:?}"
        );
        assert!(
            paras[1].contains("一直读到天黑为止"),
            "two CJK lines must join without a space: {}",
            paras[1]
        );
    }

    #[test]
    fn hyphenation_is_undone_but_compounds_survive() {
        assert_eq!(
            join_lines("this is an exam-", "ple of hyphenation"),
            "this is an example of hyphenation",
            "a hyphen before a lowercase fragment was the typesetter's"
        );
        assert_eq!(
            join_lines("relations with Sino-", "American trade"),
            "relations with Sino-American trade",
            "a hyphen before a capital belongs to the word"
        );
        assert_eq!(
            join_lines("ends with a dash -", "and continues"),
            "ends with a dash - and continues",
            "a dangling dash is not hyphenation"
        );
    }

    #[test]
    fn a_short_last_line_closes_a_paragraph() {
        let lines = page(&[
            "这一段的第一行排满了整个版面，看起来长度是很正常的样子",
            "第二行同样排满了版面，读起来和上一行差不多长的样子啊",
            "只剩几个字",
            "下一段又从满行开始，长度回到和前面两行相当的样子吧",
            "第二行仍旧是满的，长度和上面那些行基本一致的样子哦",
        ]);
        let paras = paragraphs(&lines);
        assert_eq!(
            paras.len(),
            2,
            "the narrow line ends the first paragraph, got {paras:?}"
        );
        assert!(
            paras[0].ends_with("只剩几个字"),
            "the short line still belongs to the paragraph it closes: {}",
            paras[0]
        );
    }

    #[test]
    fn a_list_starts_its_own_paragraph() {
        let lines = page(&[
            "准备工作分成下面几步，缺一步都不行",
            "• 先把书导入书库",
            "• 再选择要读的那一本",
            "1. 数字列表也算新的一块",
            "（2）括号编号同样如此",
        ]);
        let paras = paragraphs(&lines);
        assert_eq!(paras.len(), 5, "each marker opens a block, got {paras:?}");
        assert!(
            paras[1].starts_with('•'),
            "the bullet must not be swallowed by the line above: {paras:?}"
        );
    }

    #[test]
    fn headings_do_not_glue_to_the_prose_around_them() {
        let lines = page(&[
            "上一节讲完了，这里是最后一句没有句号的话",
            "第二章 命名",
            "新的一章从这里开始，第一行的长度看起来是正常的",
        ]);
        let paras = paragraphs(&lines);
        assert_eq!(paras.len(), 3, "the heading stands alone, got {paras:?}");
        assert_eq!(paras[1], "第二章 命名");
    }

    #[test]
    fn heading_detection_refuses_prose() {
        for line in ["第三章", "第三章 命名", "Chapter 4", "3.1 方法", "目录"] {
            assert!(is_heading_like(line), "should be a heading: {line}");
        }
        for line in [
            "第一章的那段话讲得很清楚",
            "第三章里提到过这件事",
            "他说：",
            "1. 买牛奶",
            "The quick brown fox jumped over the lazy dog again",
            "chapter and verse were quoted at length by the speaker",
        ] {
            assert!(!is_heading_like(line), "should not be a heading: {line}");
        }
    }

    #[test]
    fn page_numbers_are_told_apart_from_content() {
        for line in [
            "12",
            "— 12 —",
            "第 12 页",
            "第12页",
            "Page 3 of 9",
            "page 3 / 9",
            "iv",
            "[ 7 ]",
        ] {
            assert!(is_page_number(line), "should be a page number: {line}");
        }
        for line in [
            "2024 年的记录",
            "1949 1976",
            "Chapter 12",
            "第 12 章",
            "readio",
        ] {
            assert!(!is_page_number(line), "should be content: {line}");
        }
    }

    #[test]
    fn running_headers_and_page_numbers_are_stripped() {
        let refrain = "他又说了一遍那句话，作者就是这么写的";
        let pages: Vec<Vec<String>> = (1..=4)
            .map(|n| {
                let body = format!("第 {n} 页的正文，长度看起来还算正常的一行字");
                page(&[
                    "readio 使用手册",
                    &body,
                    refrain,
                    &body,
                    refrain,
                    &body,
                    "读到这里这一页就结束了。",
                    &format!("- {n} -"),
                ])
            })
            .collect();
        let stripped = strip_boilerplate(&pages);

        for (index, lines) in stripped.iter().enumerate() {
            assert!(
                !lines.iter().any(|l| l.contains("使用手册")),
                "page {} kept its running header: {lines:?}",
                index + 1
            );
            assert!(
                !lines
                    .iter()
                    .any(|l| l.contains(char::is_numeric) && l.contains('-')),
                "page {} kept its page number: {lines:?}",
                index + 1
            );
            assert!(
                lines.iter().filter(|l| *l == refrain).count() == 2,
                "a line repeated inside the body is content, not furniture: {lines:?}"
            );
        }
    }

    #[test]
    fn a_two_page_document_keeps_its_headers() {
        let pages: Vec<Vec<String>> = (1..=2)
            .map(|_| page(&["同一个页眉", "正文在这里，长度看起来还算正常的一行字"]))
            .collect();
        let stripped = strip_boilerplate(&pages);
        assert!(
            stripped[0].iter().any(|l| l == "同一个页眉"),
            "two pages cannot establish a habit, so nothing should be dropped: {:?}",
            stripped[0]
        );
    }

    #[test]
    fn a_paragraph_split_by_a_page_break_is_rejoined() {
        let pages = vec![
            page(&[
                "第一页的第一行写满了整个版面看起来长度非常正常的样子哦",
                "第二行也是满的，长度和上面那一行基本一致的样子啊啊",
                "第三行同样写满了版面，这一句还没有说完就翻页了呀呀",
            ]),
            page(&[
                "接着上一页说下去，这一行的长度和前面几行差不多的样子",
                "第二行仍然是满的，长度看起来和上面那些行一致的样子",
                "最后一行到这里就结束了。",
            ]),
        ];
        let blocks = blocks(&pages);
        let spanning = blocks
            .iter()
            .find(|b| b.text.contains("就翻页了呀呀接着上一页说下去"))
            .unwrap_or_else(|| panic!("the sentence should span the page break: {blocks:?}"));
        assert_eq!(
            (spanning.page, spanning.through),
            (1, 2),
            "a stitched paragraph has to remember both pages it came from"
        );
        assert!(
            blocks.iter().all(|b| b.page >= 1 && b.through <= 2),
            "every block must remember a real page: {blocks:?}"
        );
    }

    #[test]
    fn a_page_break_after_a_full_stop_is_respected() {
        let pages = vec![
            page(&[
                "第一页最后一句在页面底部就说完了，这里是句号。",
                "上面那行虽然写满了版面，可句子已经收尾了呀呀呀啊",
                "所以下一页不应该被接到这一段的后面来的呀呀呀。",
            ]),
            page(&[
                "第二页另起一段，长度看起来和前一页的行差不多的样子",
                "第二行同样是满的，长度和上面那一行基本一致的样子",
                "到这里结束。",
            ]),
        ];
        let blocks = blocks(&pages);
        assert!(
            !blocks
                .iter()
                .any(|b| b.text.contains("呀呀呀。第二页另起一段")),
            "a closed sentence must not absorb the next page: {blocks:?}"
        );
    }

    #[test]
    fn pages_are_grouped_into_chapters_with_page_ranges() {
        let filler: String = "字".repeat(1200);
        let blocks: Vec<Block> = (1..=6).map(|page| text_block(page, &filler)).collect();
        let chapters = assemble(&page_groups(&blocks), "手册.pdf");

        assert!(
            chapters.len() >= 2,
            "7200 characters should make more than one chapter, got {}",
            chapters.len()
        );
        for chapter in &chapters {
            assert!(
                !chapter.title.trim().is_empty(),
                "a chapter without a title cannot be shown"
            );
            assert!(
                !chapter.paras.is_empty(),
                "{} has no paragraphs",
                chapter.title
            );
            assert!(
                chapter.href.starts_with("手册.pdf#p"),
                "the href should cite the source pages: {}",
                chapter.href
            );
        }
        assert!(
            chapters[0].title.contains("(p1-3)") && chapters[0].title.contains('1'),
            "the fallback title should name its page range, got {:?}",
            chapters[0].title
        );
        assert_eq!(
            chapters.iter().map(|c| c.paras.len()).sum::<usize>(),
            blocks.len(),
            "grouping must not drop or duplicate paragraphs"
        );
    }

    #[test]
    fn a_stub_at_the_end_joins_the_previous_chapter() {
        let mut blocks: Vec<Block> = (1..=6)
            .map(|page| text_block(page, &"字".repeat(1200)))
            .collect();
        blocks.push(text_block(7, "尾巴"));
        let chapters = assemble(&page_groups(&blocks), "b.pdf");
        assert!(
            chapters
                .last()
                .is_some_and(|c| c.paras.iter().any(|p| p.text() == "尾巴")),
            "the stub should ride along, not become a chapter of its own: {:?}",
            chapters.iter().map(|c| c.title.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn headings_split_the_book_when_prose_follows_them() {
        let blocks = vec![
            heading_block(1, "第一章 开场"),
            text_block(1, "开场的正文。"),
            heading_block(3, "第二章 收场"),
            text_block(3, "收场的正文。"),
        ];
        let groups = heading_groups(&blocks).expect("two headings with prose should split");
        let chapters = assemble(&groups, "b.pdf");
        assert_eq!(
            chapters.iter().map(|c| c.title.clone()).collect::<Vec<_>>(),
            vec!["第一章 开场", "第二章 收场"]
        );
        assert_eq!(
            chapters[1].href, "b.pdf#p3",
            "a single-page chapter cites one page"
        );
    }

    #[test]
    fn a_table_of_contents_does_not_become_a_shelf_of_chapters() {
        let mut blocks: Vec<Block> = (1..=6)
            .map(|n| heading_block(1, &format!("第{n}章")))
            .collect();
        blocks.push(text_block(2, "真正的正文从第二页开始。"));
        assert!(
            heading_groups(&blocks).is_none(),
            "headings without prose between them are a table of contents"
        );
    }
}
