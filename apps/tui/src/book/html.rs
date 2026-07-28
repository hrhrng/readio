//! XHTML → paragraphs.
//!
//! Deliberately forgiving: EPUB files in the wild carry unbalanced tags,
//! HTML-only entities and inline markup we have no use for. We stream events,
//! keep the text of block-level elements, and drop everything else.

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{Emphasis, Para, Rich};

/// Inline tags worth keeping. A book that italicises a ship's name is saying
/// something about the ship's name, and dropping it loses the sentence's shape.
fn emphasis_kind(tag: &str) -> Option<bool> {
    match tag {
        "em" | "i" | "cite" | "dfn" | "var" => Some(false),
        "strong" | "b" => Some(true),
        _ => None,
    }
}

/// Tags whose text we collect, with the kind of paragraph they produce.
fn block_kind(tag: &str) -> Option<Block> {
    match tag {
        "h1" => Some(Block::Heading(1)),
        "h2" => Some(Block::Heading(2)),
        "h3" => Some(Block::Heading(3)),
        "h4" | "h5" | "h6" => Some(Block::Heading(4)),
        "p" | "div" | "li" | "dd" | "dt" | "figcaption" => Some(Block::Text),
        "blockquote" => Some(Block::Quote),
        "pre" | "code" => Some(Block::Code),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Heading(u8),
    Text,
    Quote,
    Code,
}

/// A parsed XHTML document: its paragraphs, plus where the anchors are.
///
/// The anchors are what makes a ToC entry like `part1.xhtml#ch3` mean anything:
/// they say which paragraph a named point in the document precedes, so the
/// loader can cut one spine file into the chapters its table of contents
/// promises.
#[derive(Debug, Default)]
pub struct Document {
    pub paras: Vec<Para>,
    pub anchors: std::collections::HashMap<String, usize>,
}

/// Convert an XHTML document body into paragraphs.
pub fn to_paras(xhtml: &str) -> Vec<Para> {
    to_document(xhtml).paras
}

/// Convert a document, honouring only the emphasis it states in tags.
pub fn to_document(xhtml: &str) -> Document {
    to_document_styled(xhtml, &super::css::Classes::new())
}

/// An open block element, with the text and the emphasis gathered inside it.
struct Open {
    tag: String,
    kind: Block,
    text: String,
    emphasis: Vec<Emphasis>,
}

/// Convert an XHTML document body into paragraphs and anchor positions.
///
/// `classes` names the CSS classes that emphasise text, which is how converted
/// EPUBs spell italics: `<span class="calibre14">` and a stylesheet, rather than
/// `<em>`.
pub fn to_document_styled(xhtml: &str, classes: &super::css::Classes) -> Document {
    let prepared = prepare(xhtml);
    let mut reader = Reader::from_str(&prepared);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.trim_text(false);

    let mut paras: Vec<Para> = Vec::new();
    // Anchor name → index of the paragraph it precedes, before deduplication
    // renumbers anything.
    let mut anchors: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut stack: Vec<Open> = Vec::new();
    // Emphasis waiting for its closing tag. The depth matters because
    // `<p><em>a</em></p>` and `<em><p>a</p></em>` are both out there, and only
    // the first is emphasis on a run of text; the tag matters because a `<span>`
    // that emphasises has to be closed by its own `</span>` and not by the next
    // one along.
    let mut pending: Vec<Pending> = Vec::new();
    let mut skip_depth = 0usize;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = local_name(e.name().as_ref());
                if matches!(tag.as_str(), "head" | "style" | "script" | "svg") {
                    skip_depth += 1;
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                note_anchors(&e, paras.len(), &mut anchors);
                if matches!(tag.as_str(), "img" | "image")
                    && let Some(para) = image_para(&e)
                {
                    flush(&mut stack, &mut paras, &mut pending);
                    paras.push(para);
                    continue;
                }
                if let Some(strong) = emphasis_kind(&tag).or_else(|| styled(&e, classes)) {
                    if let Some(open) = stack.last() {
                        pending.push(Pending {
                            depth: stack.len(),
                            start: open.text.len(),
                            strong,
                            tag: tag.clone(),
                        });
                    }
                    if block_kind(&tag).is_none() {
                        continue;
                    }
                }
                if let Some(kind) = block_kind(&tag) {
                    stack.push(Open {
                        tag,
                        kind,
                        text: String::new(),
                        emphasis: Vec::new(),
                    });
                }
            }
            Ok(Event::End(e)) => {
                let tag = local_name(e.name().as_ref());
                if matches!(tag.as_str(), "head" | "style" | "script" | "svg") {
                    skip_depth = skip_depth.saturating_sub(1);
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                // A tag can both hold text and emphasise it (`<p class="i">`),
                // so closing the run comes first and closing the block after.
                close_emphasis(&mut stack, &mut pending, &tag);
                if emphasis_kind(&tag).is_some() && block_kind(&tag).is_none() {
                    continue;
                }
                // Close the innermost matching block; tolerate stray ends.
                if let Some(pos) = stack.iter().rposition(|open| open.tag == tag) {
                    let open = stack.remove(pos);
                    pending.retain(|p| p.depth <= pos);
                    if let Some(para) = finish(open.kind, open.text, open.emphasis) {
                        paras.push(para);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if skip_depth > 0 {
                    continue;
                }
                let tag = local_name(e.name().as_ref());
                note_anchors(&e, paras.len(), &mut anchors);
                if matches!(tag.as_str(), "img" | "image") {
                    // An illustration interrupts the paragraph it sits in: emit
                    // the text so far, then the picture, so the order on screen
                    // matches the order in the document.
                    if let Some(para) = image_para(&e) {
                        flush(&mut stack, &mut paras, &mut pending);
                        paras.push(para);
                        continue;
                    }
                }
                if matches!(tag.as_str(), "br" | "img" | "image")
                    && let Some(open) = stack.last_mut()
                {
                    open.text.push(' ');
                }
            }
            Ok(Event::Text(e)) => {
                if skip_depth > 0 {
                    continue;
                }
                let decoded = e
                    .decode()
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).into_owned());
                if let Some(open) = stack.last_mut() {
                    open.text.push_str(&decoded);
                }
            }
            Ok(Event::CData(e)) => {
                if skip_depth == 0 {
                    let raw = String::from_utf8_lossy(e.as_ref()).into_owned();
                    if let Some(open) = stack.last_mut() {
                        open.text.push_str(&raw);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            // A malformed document should degrade, not abort: keep what we have.
            Err(_) => break,
        }
        buf.clear();
    }

    // Flush anything left open by an unbalanced document.
    for open in stack.into_iter() {
        if let Some(para) = finish(open.kind, open.text, open.emphasis) {
            paras.push(para);
        }
    }

    let (paras, moved) = dedupe(paras);
    // Deduplication renumbers paragraphs, so the anchors have to follow. An
    // anchor past the end is kept clamped: it names a point with no content
    // after it, and the loader drops the empty piece rather than guessing.
    let limit = paras.len();
    for index in anchors.values_mut() {
        *index = moved.get(*index).copied().unwrap_or(limit).min(limit);
    }
    Document { paras, anchors }
}

/// Record `id` and `<a name=…>` as pointing at the paragraph that follows.
fn note_anchors(
    e: &quick_xml::events::BytesStart<'_>,
    next_para: usize,
    anchors: &mut std::collections::HashMap<String, usize>,
) {
    for attr in e.attributes().flatten() {
        if !matches!(local_name(attr.key.as_ref()).as_str(), "id" | "name") {
            continue;
        }
        let Ok(value) = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
            continue;
        };
        let name = value.trim().to_string();
        if name.is_empty() {
            continue;
        }
        // First one wins: a duplicated id is malformed, and the earlier
        // position is the one the table of contents was written against.
        anchors.entry(name).or_insert(next_para);
    }
}

/// An emphasis run whose closing tag has not arrived yet.
struct Pending {
    depth: usize,
    start: usize,
    strong: bool,
    tag: String,
}

/// Record an emphasis run that just closed, if it was a run of text inside the
/// block that is still open.
fn close_emphasis(stack: &mut [Open], pending: &mut Vec<Pending>, tag: &str) {
    let depth = stack.len();
    let Some(index) = pending
        .iter()
        .rposition(|p| p.depth == depth && p.tag == tag)
    else {
        return;
    };
    let run = pending.remove(index);
    if let Some(open) = stack.last_mut()
        && open.text.len() > run.start
    {
        open.emphasis.push(Emphasis {
            start: run.start as u32,
            end: open.text.len() as u32,
            strong: run.strong,
        });
    }
}

/// Whether an element emphasises through CSS: a class the stylesheet leans on,
/// or an inline `style` that says so outright.
fn styled(e: &quick_xml::events::BytesStart<'_>, classes: &super::css::Classes) -> Option<bool> {
    let mut found: Option<bool> = None;
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        let Ok(value) = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
            continue;
        };
        match key.as_str() {
            "class" => {
                for name in value.split_whitespace() {
                    if let Some(strong) = classes.get(name) {
                        found = Some(found.unwrap_or(false) || *strong);
                    }
                }
            }
            "style" => {
                if let Some(strong) = super::css::weight(&value) {
                    found = Some(found.unwrap_or(false) || strong);
                }
            }
            _ => {}
        }
    }
    found
}

/// Close the innermost open block, so a picture lands after the text that
/// preceded it rather than before it.
fn flush(stack: &mut Vec<Open>, paras: &mut Vec<Para>, pending: &mut Vec<Pending>) {
    if let Some(open) = stack.pop() {
        let (tag, kind) = (open.tag, open.kind);
        if let Some(para) = finish(kind, open.text, open.emphasis) {
            paras.push(para);
        }
        // Keep the element open with an empty buffer: its remaining text is a
        // new paragraph, which is exactly how it reads on the page. Any emphasis
        // still open was measured against the text just emitted, so its offsets
        // mean nothing now.
        pending.retain(|p| p.depth < stack.len() + 1);
        stack.push(Open {
            tag,
            kind,
            text: String::new(),
            emphasis: Vec::new(),
        });
    }
}

/// `<img src=… alt=…>` → an image paragraph, with the href left unresolved:
/// only the EPUB loader knows where the archive put the file.
fn image_para(e: &quick_xml::events::BytesStart<'_>) -> Option<Para> {
    let mut src = String::new();
    let mut alt = String::new();
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map(|v| v.into_owned())
            .unwrap_or_default();
        match key.as_str() {
            // `xlink:href` is how SVG-wrapped covers point at their bitmap.
            "src" | "href" => src = value,
            // `alt` is the caption; `title` only fills in when there is none.
            "alt" | "title" if alt.is_empty() => alt = collapse_ws(&value),
            _ => {}
        }
    }
    if src.trim().is_empty() {
        return None;
    }
    Some(Para::Image {
        src: std::path::PathBuf::from(src),
        alt,
    })
}

fn finish(kind: Block, text: String, emphasis: Vec<Emphasis>) -> Option<Para> {
    if kind == Block::Code {
        let cleaned = text.trim_end().trim_start_matches('\n').to_string();
        return (!cleaned.trim().is_empty()).then_some(Para::Code(cleaned));
    }
    // Collapsing whitespace moves every byte after the first run of it, so the
    // emphasis offsets are carried across with a map rather than recomputed.
    let (cleaned, map) = collapse_ws_indexed(&text);
    if cleaned.trim().is_empty() {
        return None;
    }
    let spans = remap(&emphasis, &map, cleaned.len());
    Some(match kind {
        Block::Heading(level) => Para::Heading {
            level,
            text: cleaned,
        },
        Block::Text => Para::Text(Rich::new(cleaned, spans)),
        Block::Quote => Para::Quote(Rich::new(cleaned, spans)),
        Block::Code => unreachable!("handled above"),
    })
}

/// Move emphasis ranges into collapsed coordinates, then tidy them: sorted,
/// clamped, empty ones dropped, touching ones of the same kind merged.
fn remap(spans: &[Emphasis], map: &[u32], len: usize) -> Vec<Emphasis> {
    let at = |raw: u32| -> usize {
        let index = (raw as usize).min(map.len().saturating_sub(1));
        (map.get(index).copied().unwrap_or(0) as usize).min(len)
    };
    let mut out: Vec<Emphasis> = Vec::with_capacity(spans.len());
    for span in spans {
        let (start, end) = (at(span.start), at(span.end));
        if end <= start {
            continue;
        }
        out.push(Emphasis {
            start: start as u32,
            end: end as u32,
            strong: span.strong,
        });
    }
    out.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<Emphasis> = Vec::with_capacity(out.len());
    for span in out {
        match merged.last_mut() {
            Some(last) if last.strong == span.strong && span.start <= last.end => {
                last.end = last.end.max(span.end);
            }
            _ => merged.push(span),
        }
    }
    merged
}

/// `<div><p>x</p></div>` yields both the inner `p` and the outer `div` with the
/// same text. Drop a paragraph whose text is contained in its predecessor.
///
/// Also reports where each input paragraph ended up, because anchors were
/// recorded against the numbering on the way in.
fn dedupe(paras: Vec<Para>) -> (Vec<Para>, Vec<usize>) {
    let mut out: Vec<Para> = Vec::with_capacity(paras.len());
    let mut moved: Vec<usize> = Vec::with_capacity(paras.len());
    for para in paras {
        // Pictures are never duplicates of prose, and two pictures with the
        // same empty alt text are still two pictures.
        if para.is_image() || out.last().is_some_and(Para::is_image) {
            moved.push(out.len());
            out.push(para);
            continue;
        }
        let is_dup = out.last().is_some_and(|prev| {
            let (a, b) = (prev.text(), para.text());
            a == b || (b.len() > 8 && a.contains(b)) || (a.len() > 8 && b.contains(a))
        });
        if is_dup {
            // Keep the longer of the two — usually the container.
            if para.char_count() > out.last().map(Para::char_count).unwrap_or(0) {
                out.pop();
                out.push(para);
            }
            // Either way the two texts are now one paragraph, and anything
            // pointing at the second should point at the survivor.
            moved.push(out.len().saturating_sub(1));
            continue;
        }
        moved.push(out.len());
        out.push(para);
    }
    (out, moved)
}

fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

fn collapse_ws(text: &str) -> String {
    collapse_ws_indexed(text).0
}

/// Collapse runs of whitespace, and report where every byte of the input ended
/// up so ranges measured against it can follow.
///
/// The map has one entry per input byte plus one past the end, which is what
/// makes a range's exclusive end translatable.
fn collapse_ws_indexed(text: &str) -> (String, Vec<u32>) {
    let mut out = String::with_capacity(text.len());
    let mut map = vec![0u32; text.len() + 1];
    let mut prev_space = false;
    for (index, c) in text.char_indices() {
        for slot in map.iter_mut().skip(index).take(c.len_utf8()) {
            *slot = out.len() as u32;
        }
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    map[text.len()] = out.len() as u32;
    // Only the tail can carry a space at this point: the leading one is never
    // pushed, so trimming the end cannot move anything the map points at.
    let trimmed = out.trim_end();
    let cut = trimmed.len() as u32;
    for slot in map.iter_mut() {
        *slot = (*slot).min(cut);
    }
    (trimmed.to_string(), map)
}

/// Strip the doctype and expand the HTML-only entities XML parsers reject.
fn prepare(xhtml: &str) -> String {
    let mut s = xhtml.to_string();
    while let Some(start) = s.find("<!DOCTYPE") {
        match s[start..].find('>') {
            Some(rel) => {
                s.replace_range(start..start + rel + 1, "");
            }
            None => break,
        }
    }
    for (from, to) in [
        ("&nbsp;", " "),
        ("&mdash;", "—"),
        ("&ndash;", "–"),
        ("&hellip;", "…"),
        ("&ldquo;", "“"),
        ("&rdquo;", "”"),
        ("&lsquo;", "‘"),
        ("&rsquo;", "’"),
        ("&copy;", "©"),
        ("&middot;", "·"),
        ("&laquo;", "«"),
        ("&raquo;", "»"),
        ("&times;", "×"),
        ("&trade;", "™"),
    ] {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

/// First heading in a document, used as a chapter title fallback.
pub fn first_heading(paras: &[Para]) -> Option<String> {
    paras.iter().find_map(|p| match p {
        Para::Heading { text, .. } => Some(text.clone()),
        _ => None,
    })
}
