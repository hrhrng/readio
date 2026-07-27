//! XHTML → paragraphs.
//!
//! Deliberately forgiving: EPUB files in the wild carry unbalanced tags,
//! HTML-only entities and inline markup we have no use for. We stream events,
//! keep the text of block-level elements, and drop everything else.

use quick_xml::Reader;
use quick_xml::events::Event;

use super::Para;

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

/// Convert an XHTML document body into paragraphs.
pub fn to_paras(xhtml: &str) -> Vec<Para> {
    let prepared = prepare(xhtml);
    let mut reader = Reader::from_str(&prepared);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.trim_text(false);

    let mut paras: Vec<Para> = Vec::new();
    // Stack of open block elements: (tag, kind, accumulated text).
    let mut stack: Vec<(String, Block, String)> = Vec::new();
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
                if matches!(tag.as_str(), "img" | "image")
                    && let Some(para) = image_para(&e)
                {
                    flush(&mut stack, &mut paras);
                    paras.push(para);
                    continue;
                }
                if let Some(kind) = block_kind(&tag) {
                    stack.push((tag, kind, String::new()));
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
                // Close the innermost matching block; tolerate stray ends.
                if let Some(pos) = stack.iter().rposition(|(t, _, _)| *t == tag) {
                    let (_, kind, text) = stack.remove(pos);
                    if let Some(para) = finish(kind, text) {
                        paras.push(para);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if skip_depth > 0 {
                    continue;
                }
                let tag = local_name(e.name().as_ref());
                if matches!(tag.as_str(), "img" | "image") {
                    // An illustration interrupts the paragraph it sits in: emit
                    // the text so far, then the picture, so the order on screen
                    // matches the order in the document.
                    if let Some(para) = image_para(&e) {
                        flush(&mut stack, &mut paras);
                        paras.push(para);
                        continue;
                    }
                }
                if matches!(tag.as_str(), "br" | "img" | "image")
                    && let Some((_, _, text)) = stack.last_mut()
                {
                    text.push(' ');
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
                if let Some((_, _, text)) = stack.last_mut() {
                    text.push_str(&decoded);
                }
            }
            Ok(Event::CData(e)) => {
                if skip_depth == 0 {
                    let raw = String::from_utf8_lossy(e.as_ref()).into_owned();
                    if let Some((_, _, text)) = stack.last_mut() {
                        text.push_str(&raw);
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
    for (_, kind, text) in stack.into_iter() {
        if let Some(para) = finish(kind, text) {
            paras.push(para);
        }
    }

    dedupe(paras)
}

/// Close the innermost open block, so a picture lands after the text that
/// preceded it rather than before it.
fn flush(stack: &mut Vec<(String, Block, String)>, paras: &mut Vec<Para>) {
    if let Some((tag, kind, text)) = stack.pop() {
        if let Some(para) = finish(kind, text) {
            paras.push(para);
        }
        // Keep the element open with an empty buffer: its remaining text is a
        // new paragraph, which is exactly how it reads on the page.
        stack.push((tag, kind, String::new()));
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

fn finish(kind: Block, text: String) -> Option<Para> {
    let cleaned = match kind {
        Block::Code => text.trim_end().trim_start_matches('\n').to_string(),
        _ => collapse_ws(&text),
    };
    if cleaned.trim().is_empty() {
        return None;
    }
    Some(match kind {
        Block::Heading(level) => Para::Heading {
            level,
            text: cleaned,
        },
        Block::Text => Para::Text(cleaned),
        Block::Quote => Para::Quote(cleaned),
        Block::Code => Para::Code(cleaned),
    })
}

/// `<div><p>x</p></div>` yields both the inner `p` and the outer `div` with the
/// same text. Drop a paragraph whose text is contained in its predecessor.
fn dedupe(paras: Vec<Para>) -> Vec<Para> {
    let mut out: Vec<Para> = Vec::with_capacity(paras.len());
    for para in paras {
        // Pictures are never duplicates of prose, and two pictures with the
        // same empty alt text are still two pictures.
        if para.is_image() || out.last().is_some_and(Para::is_image) {
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
            continue;
        }
        out.push(para);
    }
    out
}

fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for c in text.chars() {
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
    out.trim().to_string()
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
