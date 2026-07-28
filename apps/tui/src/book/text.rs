//! Plain text and Markdown loading.
//!
//! Chapters are split on Markdown `#`/`##` headings, or on a blank-line run
//! followed by a short standalone line — the shape most `.txt` books use for
//! chapter markers.

use std::path::Path;

use anyhow::{Context, Result};

use super::{Book, Chapter, Para, Source};

pub fn load(path: &Path, assets: Option<&Path>) -> Result<Book> {
    let raw = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
    let href = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book.txt".to_string());

    let mut chapters = parse(&text, &href);
    // Markdown image paths are relative to the file, and the reader may open a
    // book from anywhere, so resolve them now.
    let base = path.parent().unwrap_or(Path::new("."));
    for chapter in chapters.iter_mut() {
        resolve_images(&mut chapter.paras, base, assets);
    }
    anyhow::ensure!(
        !chapters.is_empty(),
        "{} has no readable text",
        path.display()
    );

    let title = chapters
        .first()
        .map(|c| c.title.clone())
        .filter(|t| chapters.len() > 1 && !t.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string())
        });

    Ok(Book {
        id: String::new(),
        title,
        author: None,
        path: Some(path.to_path_buf()),
        source: Source::Text,
        chapters,
        cover: None,
    })
}

/// `![alt](path)` on a line of its own → an illustration.
///
/// Only a whole line counts: an image reference inside a sentence is left as
/// text, because breaking a paragraph in half around a small inline icon reads
/// worse than the markup does.
fn markdown_image(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("![")?;
    let (alt, rest) = rest.split_once("](")?;
    let src = rest.strip_suffix(')')?;
    if src.trim().is_empty() {
        return None;
    }
    // A title after the path (`![a](b "c")`) is not something we show.
    let src = src.split_once(" \"").map(|(p, _)| p).unwrap_or(src);
    Some((alt.trim().to_string(), src.trim().to_string()))
}

/// Make image paths absolute against the book's own directory, and turn the
/// ones that point at nothing into their captions.
fn resolve_images(paras: &mut Vec<Para>, base: &Path, assets: Option<&Path>) {
    paras.retain_mut(|para| {
        let Para::Image { src, alt } = para else {
            return true;
        };
        if src.is_absolute() && src.is_file() {
            return true;
        }
        // Next to the book first, then where the book came from.
        let found = [Some(base), assets]
            .into_iter()
            .flatten()
            .map(|dir| dir.join(&*src))
            .find(|candidate| candidate.is_file());
        if let Some(candidate) = found {
            *src = candidate;
            return true;
        }
        if alt.trim().is_empty() {
            false
        } else {
            *para = Para::Text(alt.clone().into());
            true
        }
    });
}

/// Split Markdown-ish plain text into chapters.
pub fn parse(text: &str, href: &str) -> Vec<Chapter> {
    let href = href.to_string();
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut current = Chapter::new(String::new(), href.clone(), Vec::new());

    let mut in_code = false;
    let mut code_buf = String::new();

    for block in text.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.trim().is_empty() {
            continue;
        }
        for line in block.split('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_code {
                    current
                        .paras
                        .push(Para::Code(std::mem::take(&mut code_buf)));
                }
                in_code = !in_code;
                continue;
            }
            if in_code {
                code_buf.push_str(line);
                code_buf.push('\n');
                continue;
            }

            if let Some(heading) = markdown_heading(trimmed) {
                let (level, title) = heading;
                if level <= 2 && !current.paras.is_empty() {
                    chapters.push(finish(current, chapters.len()));
                    current = Chapter::new(title.clone(), href.clone(), Vec::new());
                }
                if current.title.is_empty() {
                    current.title = title.clone();
                }
                current.paras.push(Para::Heading { level, text: title });
                continue;
            }
            if let Some((alt, src)) = markdown_image(trimmed) {
                current.paras.push(Para::Image {
                    src: std::path::PathBuf::from(src),
                    alt,
                });
                continue;
            }
            if trimmed.starts_with('>') {
                current
                    .paras
                    .push(Para::Quote(trimmed.trim_start_matches('>').trim().into()));
                continue;
            }
            if is_bare_chapter_marker(trimmed) && !current.paras.is_empty() {
                chapters.push(finish(current, chapters.len()));
                current = Chapter::new(
                    trimmed.to_string(),
                    href.clone(),
                    vec![Para::Heading {
                        level: 2,
                        text: trimmed.to_string(),
                    }],
                );
                continue;
            }
            current.paras.push(Para::Text(trimmed.into()));
        }
    }
    if in_code && !code_buf.is_empty() {
        current.paras.push(Para::Code(code_buf));
    }
    if !current.paras.is_empty() {
        chapters.push(finish(current, chapters.len()));
    }
    chapters
}

fn finish(mut chapter: Chapter, index: usize) -> Chapter {
    if chapter.title.is_empty() {
        chapter.title = crate::i18n::tf("book.section", &[&(index + 1)]);
    }
    chapter
}

fn markdown_heading(line: &str) -> Option<(u8, String)> {
    if !line.starts_with('#') {
        return None;
    }
    let level = line.chars().take_while(|c| *c == '#').count().min(6) as u8;
    let title = line.trim_start_matches('#').trim().to_string();
    (!title.is_empty()).then_some((level, title))
}

/// A short standalone line that reads like a chapter label: `第三章`,
/// `第三章 命名`, `Chapter 4`, `PART II`.
///
/// The rule is deliberately strict. Prose often opens with `第一章的那段话…`,
/// and splitting there would invent a chapter; refusing to split merely leaves
/// two sections joined, which is the cheaper mistake.
fn is_bare_chapter_marker(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() || chars.len() > 24 {
        return false;
    }
    // A sentence, not a label.
    if line.ends_with(['。', '，', '、', '：', '；', '!', '！', '?', '？', ',']) {
        return false;
    }
    let lower = line.to_lowercase();
    if lower.starts_with("chapter ") || lower.starts_with("part ") {
        return true;
    }
    if chars[0] != '第' {
        return false;
    }
    // 第 + numeral + unit, with nothing but a separator or a title after it.
    let Some(unit_at) = chars.iter().position(|c| "章节卷回".contains(*c)) else {
        return false;
    };
    let numeral = "0123456789零〇一二三四五六七八九十百千两 ";
    let number_ok = unit_at > 1 && chars[1..unit_at].iter().all(|c| numeral.contains(*c));
    let tail_ok = chars
        .get(unit_at + 1)
        .is_none_or(|c| c.is_whitespace() || "：:·-—、.".contains(*c));
    number_ok && tail_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_chapter_labels() {
        for line in [
            "第三章",
            "第三章 命名",
            "第 12 节",
            "第一回：起",
            "Chapter 4",
            "PART II",
        ] {
            assert!(is_bare_chapter_marker(line), "should be a label: {line}");
        }
    }

    #[test]
    fn does_not_split_prose_that_merely_starts_with_a_numeral() {
        for line in [
            "第一章的一段话",
            "第一章的一段话。",
            "第二章讲的是注意力",
            "第三章里提到过这件事",
            "这一章很短",
        ] {
            assert!(!is_bare_chapter_marker(line), "should be prose: {line}");
        }
    }

    #[test]
    fn splits_a_plain_text_book_on_labels_only() {
        let src = "第一章\n\n第一章的开头讲了很多。\n\n第二章 命名\n\n继续。\n";
        let chapters = parse(src, "b.txt");
        assert_eq!(
            chapters.len(),
            2,
            "titles: {:?}",
            chapters.iter().map(|c| c.title.clone()).collect::<Vec<_>>()
        );
        assert_eq!(chapters[1].title, "第二章 命名");
    }
}
