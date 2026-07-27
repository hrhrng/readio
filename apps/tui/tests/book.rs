//! Parsing and reading-logic tests. The EPUB case builds a real archive in a
//! temp dir so the container → OPF → spine → XHTML path is exercised for real.

use std::io::Write;

use readio::app::flow::{self, Pos};
use readio::book::{Book, Para, html, text};

// ── XHTML ────────────────────────────────────────────────────────────────────

#[test]
fn html_keeps_block_text_and_drops_chrome() {
    let doc = r#"<?xml version="1.0" encoding="utf-8"?>
        <!DOCTYPE html>
        <html xmlns="http://www.w3.org/1999/xhtml">
        <head><title>ignored</title><style>p { color: red }</style></head>
        <body>
          <h1>第一章</h1>
          <div><p>正文<em>第一段</em>&mdash;&nbsp;带实体。</p></div>
          <blockquote>引用一句</blockquote>
          <pre>let x = 1;</pre>
          <script>var a = 1;</script>
          <p>未闭合段落
        </body></html>"#;

    let paras = html::to_paras(doc);
    let kinds: Vec<&Para> = paras.iter().collect();

    assert!(
        matches!(kinds[0], Para::Heading { level: 1, text } if text == "第一章"),
        "first block should be the h1, got {:?}",
        kinds[0]
    );
    let body = paras.iter().find_map(|p| match p {
        Para::Text(t) if t.contains("正文") => Some(t.clone()),
        _ => None,
    });
    let body = body.expect("body paragraph");
    assert!(
        body.contains("正文第一段"),
        "inline tags should flatten: {body}"
    );
    assert!(body.contains('—'), "&mdash; should decode: {body}");

    assert!(
        paras
            .iter()
            .any(|p| matches!(p, Para::Quote(t) if t == "引用一句")),
        "blockquote missing"
    );
    assert!(
        paras
            .iter()
            .any(|p| matches!(p, Para::Code(t) if t.contains("let x"))),
        "pre missing"
    );
    assert!(
        !paras.iter().any(|p| p.text().contains("var a")),
        "script content must be dropped"
    );
    assert!(
        !paras.iter().any(|p| p.text().contains("color: red")),
        "style content must be dropped"
    );
    assert!(
        paras.iter().any(|p| p.text().contains("未闭合段落")),
        "unbalanced tail should still be recovered"
    );
}

#[test]
fn html_does_not_duplicate_nested_containers() {
    let doc = "<body><div><p>只出现一次的文字</p></div></body>";
    let paras = html::to_paras(doc);
    let hits = paras
        .iter()
        .filter(|p| p.text().contains("只出现一次"))
        .count();
    assert_eq!(hits, 1, "div + p wrapping should collapse: {paras:?}");
}

// ── plain text ───────────────────────────────────────────────────────────────

#[test]
fn text_splits_on_headings_and_bare_markers() {
    let src = "# 卷首\n\n一段话。\n\n## 第一章 开始\n\n内容甲。\n\n第二章\n\n内容乙。\n";
    let chapters = text::parse(src, "book.txt");

    assert_eq!(chapters.len(), 3, "got titles {:?}", titles(&chapters));
    assert_eq!(titles(&chapters), vec!["卷首", "第一章 开始", "第二章"]);
    assert!(chapters[1].paras.iter().any(|p| p.text() == "内容甲。"));
}

#[test]
fn text_preserves_fenced_code() {
    let src = "# 标题\n\n```\nfn main() {}\n```\n\n收尾。\n";
    let chapters = text::parse(src, "book.md");
    let code = chapters[0]
        .paras
        .iter()
        .find_map(|p| match p {
            Para::Code(c) => Some(c.clone()),
            _ => None,
        })
        .expect("code block");
    assert!(code.contains("fn main"), "code lost: {code}");
}

// ── book arithmetic ──────────────────────────────────────────────────────────

#[test]
fn progress_and_search_agree_with_the_content() {
    let book = Book::load(None).expect("sample");
    assert!(book.chapters.len() >= 4);
    assert_eq!(book.progress(0, 0), 0.0);

    let last = book.chapters.len() - 1;
    let end = book.chapters[last].paras.len();
    assert!(
        (book.progress(last, end) - 1.0).abs() < 1e-6,
        "reading everything should be 100%, got {}",
        book.progress(last, end)
    );

    // The needle comes out of the book itself: the sample follows the interface
    // language, and book arithmetic should not care which one is in force.
    let source = book.chapters[1].paras[1].text();
    let needle: String = source.chars().skip(2).take(6).collect();
    let hits = book.search(&needle, 10);
    assert!(
        !hits.is_empty(),
        "searching {needle:?} taken from the text itself should hit"
    );
    // Search is case-insensitive, so compare that way rather than byte for byte.
    let lowered = needle.to_lowercase();
    for hit in &hits.shown {
        let text = book.chapters[hit.chapter].paras[hit.para].text();
        assert!(
            text.to_lowercase().contains(&lowered),
            "hit should point at the matching paragraph"
        );
        assert!(
            hit.excerpt.to_lowercase().contains(&lowered),
            "excerpt should carry the match"
        );
        // The range has to land on the match itself, since it drives the
        // highlight when the reader jumps there.
        assert_eq!(
            text[hit.range.0..hit.range.1].to_lowercase(),
            lowered,
            "the range should cover exactly the term"
        );
    }
    assert!(
        hits.total >= hits.shown.len() && hits.paragraphs > 0,
        "totals should be counted, not inferred from the shown list"
    );
}

// ── reading flow ─────────────────────────────────────────────────────────────

#[test]
fn continue_reading_advances_and_stays_inside_the_chapter() {
    let book = Book::load(None).expect("sample");
    let mut pos = Pos::default();
    let mut turns = 0;
    let mut finished = false;

    while turns < 200 {
        let (steps, next) = flow::continue_reading(&book, pos, false);
        assert!(!steps.is_empty(), "a turn should always produce steps");

        if next == pos {
            // The only fixed point is the end of the book.
            assert!(
                steps.iter().any(|s| matches!(
                    s,
                    readio::app::turn::Step::Event(readio::ui::block::Event::BookComplete)
                )),
                "a turn that does not advance must report completion"
            );
            finished = true;
            break;
        }
        assert!(
            (next.chapter, next.para) > (pos.chapter, pos.para),
            "position must move forward: {pos:?} → {next:?}"
        );
        assert!(
            next.para
                <= book.chapters[next.chapter.min(book.chapters.len() - 1)]
                    .paras
                    .len(),
            "paragraph index must stay in range: {next:?}"
        );
        pos = next;
        turns += 1;
    }

    assert!(finished, "reading should terminate, stopped at {pos:?}");
    assert!(turns > 3, "the sample should take several turns to read");
}

#[test]
fn a_chapter_boundary_does_not_waste_a_turn() {
    let book = Book::load(None).expect("sample");
    let first_len = book.chapters[0].paras.len();
    // Parked exactly at the end of chapter 1.
    let (steps, next) = flow::continue_reading(
        &book,
        Pos {
            chapter: 0,
            para: first_len,
        },
        false,
    );
    assert_eq!(next.chapter, 1, "should roll into chapter 2");
    assert!(next.para > 0, "and actually read some of it");
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, readio::app::turn::Step::Say(_))),
        "a boundary turn must still stream content"
    );
}

#[test]
fn a_read_turn_reports_a_real_line_range() {
    let book = Book::load(None).expect("sample");
    let (steps, _) = flow::continue_reading(
        &book,
        Pos {
            chapter: 1,
            para: 2,
        },
        false,
    );
    let detail = steps
        .iter()
        .find_map(|step| match step {
            readio::app::turn::Step::Tool { tool, .. } => tool.detail.clone(),
            _ => None,
        })
        .expect("a tool call with a detail");
    let range = detail.trim_start_matches('L');
    let (start, end) = range.split_once('-').expect("L<start>-<end>");
    let (start, end): (usize, usize) = (start.parse().unwrap(), end.parse().unwrap());
    assert!(start >= 1 && end > start, "bad line range {detail}");
}

#[test]
fn resuming_mentions_the_saved_position() {
    let book = Book::load(None).expect("sample");
    let (steps, _) = flow::continue_reading(
        &book,
        Pos {
            chapter: 2,
            para: 1,
        },
        true,
    );
    let thought = steps
        .iter()
        .find_map(|s| match s {
            readio::app::turn::Step::Think(t) => Some(t.clone()),
            _ => None,
        })
        .expect("a thought");
    // Chapter 3, paragraph 2, however the current language words it.
    assert!(
        thought.contains('3') && thought.contains('2'),
        "resume thought should cite the position: {thought}"
    );
}

#[test]
fn line_numbers_match_the_shape_of_the_source() {
    // A single-file book: lines accumulate across chapters.
    let sample = Book::load(None).expect("sample");
    let first = sample.line_of(0, 0);
    let later = sample.line_of(2, 0);
    assert_eq!(first, 1, "the book starts at line 1");
    assert!(
        later > sample.chapters[0].line_count(),
        "chapter 3 should start past chapter 1, got {later}"
    );
    let mut previous = 0;
    for (ci, chapter) in sample.chapters.iter().enumerate() {
        for pi in 0..chapter.paras.len() {
            let line = sample.line_of(ci, pi);
            assert!(
                line > previous,
                "lines must increase monotonically: {line} after {previous}"
            );
            previous = line;
        }
    }
}

#[test]
fn epub_chapters_each_count_from_line_one() {
    let dir = std::env::temp_dir().join(format!("readio-lines-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("tiny.epub");
    write_epub(&path);
    let book = Book::load(Some(&path)).expect("load epub");

    assert_eq!(book.line_of(0, 0), 1);
    assert_eq!(
        book.line_of(1, 0),
        1,
        "each EPUB document is its own file, so it restarts at line 1"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── EPUB ─────────────────────────────────────────────────────────────────────

#[test]
fn epub_round_trip() {
    let dir = std::env::temp_dir().join(format!("readio-epub-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("tiny.epub");
    write_epub(&path);

    let book = Book::load(Some(&path)).expect("load epub");
    assert_eq!(book.title, "小书");
    assert_eq!(book.author.as_deref(), Some("某人"));
    assert_eq!(book.chapters.len(), 2, "spine order should be honoured");
    assert_eq!(book.chapters[0].title, "开场");
    assert_eq!(book.chapters[1].title, "收场");
    assert!(book.chapters[0].href.ends_with("c1.xhtml"));
    assert!(
        book.chapters[0]
            .paras
            .iter()
            .any(|p| p.text().contains("第一章的内容")),
        "chapter body missing: {:?}",
        book.chapters[0].paras
    );
    assert!(book.uri_root().starts_with("epub://"));
    assert!(!book.id.is_empty(), "an id is needed for progress storage");

    let _ = std::fs::remove_dir_all(&dir);
}

fn write_epub(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("create epub");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let entries: &[(&str, &str)] = &[
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
                 <rootfiles><rootfile full-path="OEBPS/content.opf"
                   media-type="application/oebps-package+xml"/></rootfiles>
               </container>"#,
        ),
        (
            "OEBPS/content.opf",
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
                 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                   <dc:title>小书</dc:title><dc:creator>某人</dc:creator>
                 </metadata>
                 <manifest>
                   <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
                   <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
                   <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
                   <item id="css" href="style.css" media-type="text/css"/>
                 </manifest>
                 <spine toc="ncx">
                   <itemref idref="c1"/><itemref idref="c2"/>
                 </spine>
               </package>"#,
        ),
        (
            "OEBPS/toc.ncx",
            r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>
                 <navPoint id="n1"><navLabel><text>开场</text></navLabel>
                   <content src="c1.xhtml"/></navPoint>
                 <navPoint id="n2"><navLabel><text>收场</text></navLabel>
                   <content src="c2.xhtml#top"/></navPoint>
               </navMap></ncx>"#,
        ),
        (
            "OEBPS/c1.xhtml",
            "<html><body><h2>忽略这个标题</h2><p>第一章的内容，够长到能被读出来。</p></body></html>",
        ),
        (
            "OEBPS/c2.xhtml",
            "<html><body><p>第二章的内容，同样值得一读。</p></body></html>",
        ),
        ("OEBPS/style.css", "p { margin: 0 }"),
    ];

    for (name, body) in entries {
        zip.start_file(*name, options).expect("start file");
        zip.write_all(body.as_bytes()).expect("write");
    }
    zip.finish().expect("finish");
}

fn titles(chapters: &[readio::book::Chapter]) -> Vec<String> {
    chapters.iter().map(|c| c.title.clone()).collect()
}
