//! End-to-end PDF tests.
//!
//! The fixtures are written by hand, byte for byte: a PDF with a Helvetica
//! content stream is small enough to build in a test, and doing so keeps the
//! suite free of both binary blobs and a PDF-writing dependency.

use std::path::{Path, PathBuf};

use readio::book::Book;

const TITLE: &str = "手册";

#[test]
fn supports_recognises_pdf_by_extension() {
    assert!(
        Book::supports(Path::new("/books/handbook.PDF")),
        "the extension check must be case-insensitive"
    );
    assert!(
        !Book::supports(Path::new("/books/scan.png")),
        "an image is not a book"
    );
}

#[test]
fn a_text_pdf_becomes_a_readable_book() {
    let dir = fixture_dir("text");
    let path = dir.join("tiny.pdf");
    let pages = [
        text_stream(PAGE_ONE),
        text_stream(PAGE_TWO),
        text_stream(PAGE_THREE),
    ];
    std::fs::write(&path, pdf(&pages, &[])).expect("write fixture");

    let book = Book::load(Some(&path)).expect("a text PDF should load");

    assert_eq!(book.title, TITLE, "the /Info title is UTF-16BE encoded");
    assert_eq!(
        book.author.as_deref(),
        Some("Zhang San"),
        "the /Info author is a plain PDFDoc string"
    );
    assert!(
        book.uri_root().starts_with("pdf://"),
        "tool-call headers need a pdf URI, got {}",
        book.uri_root()
    );
    assert!(!book.id.is_empty(), "progress storage needs a content id");

    let titles: Vec<String> = book.chapters.iter().map(|c| c.title.clone()).collect();
    assert_eq!(
        titles,
        vec!["Chapter One", "Chapter Two", "Chapter Three"],
        "the heading lines should split the book"
    );
    for chapter in &book.chapters {
        assert!(
            !chapter.title.trim().is_empty() && !chapter.paras.is_empty(),
            "every chapter needs a title and some text: {chapter:?}"
        );
        assert!(
            chapter.href.starts_with("tiny.pdf#p"),
            "the href must cite the pages it came from, got {}",
            chapter.href
        );
    }
    assert_eq!(
        book.chapters[0].href, "tiny.pdf#p1-2",
        "chapter one runs over the page break"
    );

    let paras = all_text(&book);
    assert!(
        paras
            .iter()
            .any(|p| p.contains("a plain example of hyphenation")),
        "`exam-` + `ple` should be repaired: {paras:#?}"
    );
    assert!(
        paras
            .iter()
            .any(|p| p.contains("the page ends and continues onto the second page")),
        "a paragraph cut by the page break should be stitched back: {paras:#?}"
    );
    assert!(
        paras.iter().any(|p| p.contains(
            "This is the first line of the opening chapter and it runs on for a while past"
        )),
        "wrapped lines should join with single spaces: {paras:#?}"
    );
    assert!(
        !paras.iter().any(|p| p.contains("readio manual")),
        "the running header should be gone: {paras:#?}"
    );
    assert!(
        !paras.iter().any(|p| p.contains('|')),
        "the page-number footer should be gone: {paras:#?}"
    );
    assert!(
        book.line_of(1, 0) > book.chapters[0].line_count(),
        "a PDF is one file, so line numbers accumulate across chapters"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bookmarks_win_over_heading_detection() {
    let dir = fixture_dir("outline");
    let path = dir.join("outlined.pdf");
    let pages = [
        text_stream(PAGE_ONE),
        text_stream(PAGE_TWO),
        text_stream(PAGE_THREE),
    ];
    std::fs::write(&path, pdf(&pages, &[(1, "序章"), (3, "终章")])).expect("write fixture");

    let book = Book::load(Some(&path)).expect("an outlined PDF should load");
    let titles: Vec<String> = book.chapters.iter().map(|c| c.title.clone()).collect();
    assert_eq!(
        titles,
        vec!["序章", "终章"],
        "the outline names the chapters even though the pages carry headings"
    );
    assert!(
        book.chapters[1].href.contains("#p3"),
        "the second bookmark starts on page 3, got {}",
        book.chapters[1].href
    );
    assert!(
        book.chapters
            .iter()
            .all(|c| c.paras.iter().any(|p| !p.text().trim().is_empty())),
        "a bookmark that collects no text should not become a chapter"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_scanned_pdf_is_refused_with_a_reason() {
    let dir = fixture_dir("scan");
    let path = dir.join("scan.pdf");
    // Pages that draw a grey box and no text at all: what a scan looks like
    // once its images are ignored.
    let blank = b"q 0.5 g 72 72 468 648 re f Q\n".to_vec();
    std::fs::write(&path, pdf(&[blank.clone(), blank], &[])).expect("write fixture");

    let error = Book::load(Some(&path)).expect_err("an image-only PDF must be refused");
    let message = format!("{error:#}");
    assert!(
        message.contains("扫描件") && message.contains("文字型 PDF"),
        "the refusal has to say why, got {message}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── fixture text ─────────────────────────────────────────────────────────────

/// A page ends mid-sentence so the next one has something to stitch onto, and
/// carries the header and footer a real book would repeat.
const PAGE_ONE: &[&str] = &[
    "readio manual",
    "Chapter One",
    "This is the first line of the opening chapter and it runs on for a while",
    "past the edge of the page, so the reader has to join it back together in",
    "one paragraph.",
    "The next paragraph carries a hyphenated word: this is a plain exam-",
    "ple of hyphenation, and the sentence keeps going until the page ends",
    "| 1 |",
];

const PAGE_TWO: &[&str] = &[
    "readio manual",
    "and continues onto the second page, where the reader expects to find it",
    "joined to the words from the previous page before the turn happened.",
    "Chapter Two",
    "The closing chapter has a few lines of its own so that the fixture looks",
    "a little like a real book.",
    "| 2 |",
];

const PAGE_THREE: &[&str] = &[
    "readio manual",
    "Chapter Three",
    "The third page exists so that a bookmark can point at it and collect a",
    "paragraph of its own, which is all the outline path needs to be tested.",
    "That is the end of it.",
    "| 3 |",
];

// ── minimal PDF writer ───────────────────────────────────────────────────────

/// One page's content stream: Helvetica, one `Tj` per typeset line.
fn text_stream(lines: &[&str]) -> Vec<u8> {
    let mut stream = String::from("BT\n/F1 12 Tf\n16 TL\n72 720 Td\n");
    for line in lines {
        let escaped = line
            .replace('\\', r"\\")
            .replace('(', r"\(")
            .replace(')', r"\)");
        stream.push_str(&format!("({escaped}) Tj\nT*\n"));
    }
    stream.push_str("ET\n");
    stream.into_bytes()
}

/// A complete PDF: one page per content stream, plus optional top-level
/// bookmarks given as (page number, title).
fn pdf(streams: &[Vec<u8>], bookmarks: &[(usize, &str)]) -> Vec<u8> {
    let count = streams.len();
    // Objects 1-4 are fixed, then a dict + stream pair per page, then the
    // outline root and its items.
    let page_id = |index: usize| 5 + index * 2;
    let outline_root = 5 + count * 2;
    let first_item = outline_root + 1;

    let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", page_id(i))).collect();
    let outlines = if bookmarks.is_empty() {
        String::new()
    } else {
        format!(" /Outlines {outline_root} 0 R")
    };

    let mut objects: Vec<Vec<u8>> = vec![
        format!("<< /Type /Catalog /Pages 2 0 R{outlines} >>").into_bytes(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            kids.join(" ")
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        format!("<< /Title {} /Author (Zhang San) >>", utf16be(TITLE)).into_bytes(),
    ];

    for (index, stream) in streams.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page_id(index) + 1
            )
            .into_bytes(),
        );
        let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        content.extend_from_slice(stream);
        content.extend_from_slice(b"\nendstream");
        objects.push(content);
    }

    if !bookmarks.is_empty() {
        objects.push(
            format!(
                "<< /Type /Outlines /First {first_item} 0 R /Last {} 0 R /Count {} >>",
                first_item + bookmarks.len() - 1,
                bookmarks.len()
            )
            .into_bytes(),
        );
        for (index, (page, title)) in bookmarks.iter().enumerate() {
            let next = if index + 1 < bookmarks.len() {
                format!(" /Next {} 0 R", first_item + index + 1)
            } else {
                String::new()
            };
            objects.push(
                format!(
                    "<< /Title {} /Parent {outline_root} 0 R /Dest [{} 0 R /Fit]{next} >>",
                    utf16be(title),
                    page_id(page - 1)
                )
                .into_bytes(),
            );
        }
    }

    serialize(&objects)
}

/// Lay the objects out with a cross-reference table whose offsets are real.
fn serialize(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A PDF text string in the UTF-16BE form, which is how CJK titles travel.
fn utf16be(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("readio-pdf-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn all_text(book: &Book) -> Vec<String> {
    book.chapters
        .iter()
        .flat_map(|c| c.paras.iter().map(|p| p.text().to_string()))
        .collect()
}
