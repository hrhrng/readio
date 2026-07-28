//! Emphasis: what the book leaned on stays leaning.
//!
//! A paragraph is kept as a plain string with byte ranges beside it, so the
//! interesting failures are all about offsets — collapsing whitespace moves
//! every byte after it, and the passage adds markers of its own — and about the
//! ranges actually reaching the screen as a modifier.

mod common;

use std::io::Write;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use readio::app::App;
use readio::book::{Book, Para, html};
use readio::library::Library;
use readio::store::Store;

/// The emphasised slices of a paragraph, in order.
fn leaning(para: &Para) -> Vec<(String, bool)> {
    para.emphasis()
        .iter()
        .map(|span| {
            (
                para.text()[span.start as usize..span.end as usize].to_string(),
                span.strong,
            )
        })
        .collect()
}

fn first_text(paras: &[Para]) -> &Para {
    paras
        .iter()
        .find(|p| matches!(p, Para::Text(_)))
        .expect("a text paragraph")
}

#[test]
fn an_italic_run_is_kept_as_a_range_not_as_markup() {
    let paras = html::to_paras(
        "<html><body><p>他读的是 <em>看不见的城市</em>，不是别的书。</p></body></html>",
    );
    let para = first_text(&paras);
    assert!(
        !para.text().contains('*') && !para.text().contains('<'),
        "the text stays plain: {}",
        para.text()
    );
    assert_eq!(leaning(para), vec![("看不见的城市".to_string(), false)]);
}

#[test]
fn strong_and_em_are_told_apart() {
    let paras = html::to_paras(
        "<html><body><p>one <em>two</em> three <strong>four</strong> five</p></body></html>",
    );
    assert_eq!(
        leaning(first_text(&paras)),
        vec![("two".to_string(), false), ("four".to_string(), true)]
    );
}

/// The offsets are measured before whitespace is collapsed, so this is where a
/// naive implementation drifts: every run of spaces before the emphasis moves it.
#[test]
fn collapsing_whitespace_does_not_move_the_emphasis() {
    let paras = html::to_paras(
        "<html><body><p>  a\n\n   lot   of   space   before <em>here</em>\n  and after.  </p></body></html>",
    );
    let para = first_text(&paras);
    assert_eq!(leaning(para), vec![("here".to_string(), false)]);
    assert!(
        para.text().starts_with("a lot of space"),
        "whitespace still collapses: {}",
        para.text()
    );
}

#[test]
fn nested_emphasis_does_not_produce_overlapping_ranges() {
    let paras = html::to_paras("<html><body><p>a <em>b <i>c</i> d</em> e</p></body></html>");
    let para = first_text(&paras);
    let spans = para.emphasis();
    assert!(
        spans.windows(2).all(|pair| pair[0].end <= pair[1].start),
        "ranges should be disjoint and sorted: {spans:?}"
    );
    assert!(
        leaning(para).iter().any(|(text, _)| text.contains('c')),
        "the inner run survives: {:?}",
        leaning(para)
    );
}

#[test]
fn an_unclosed_tag_is_not_a_crash_and_not_a_stray_range() {
    let paras = html::to_paras("<html><body><p>a <em>b<p>c</p></body></html>");
    for para in &paras {
        for span in para.emphasis() {
            assert!(
                (span.end as usize) <= para.text().len() && span.start < span.end,
                "a range must be inside the text it belongs to: {span:?} in {:?}",
                para.text()
            );
        }
    }
}

#[test]
fn code_and_headings_carry_no_emphasis() {
    let paras = html::to_paras(
        "<html><body><h2>A <em>title</em></h2><pre>let <b>x</b> = 1;</pre></body></html>",
    );
    for para in &paras {
        assert!(
            para.emphasis().is_empty(),
            "headings are already emphatic and code means what it says: {para:?}"
        );
    }
}

/// How a converted EPUB actually spells italics. The book this was written
/// against — a Calibre-produced Calvino — contains no `<em>` at all: 488
/// `<span class="…">` and a stylesheet.
#[test]
fn italics_written_as_a_css_class_are_still_italics() {
    let classes = readio::book::css::scan(
        ".calibre14 { font-style: italic }\n.calibre9 { font-weight: bold }",
    );
    let paras = html::to_document_styled(
        "<html><body><p>他读的是 <span class=\"calibre14\">看不见的城市</span>，\
         而不是 <span class=\"calibre9\">别的书</span>。</p></body></html>",
        &classes,
    )
    .paras;
    assert_eq!(
        leaning(first_text(&paras)),
        vec![
            ("看不见的城市".to_string(), false),
            ("别的书".to_string(), true)
        ]
    );
}

#[test]
fn an_inline_style_needs_no_stylesheet() {
    let paras = html::to_paras(
        "<html><body><p>a <span style=\"font-style: italic\">b</span> c</p></body></html>",
    );
    assert_eq!(leaning(first_text(&paras)), vec![("b".to_string(), false)]);
}

/// A span that only changes colour is not emphasis, and a class that cancels an
/// inherited style is the opposite of it.
#[test]
fn ordinary_spans_stay_ordinary() {
    let classes = readio::book::css::scan(".c { color: #333 } .plain { font-style: normal }");
    let paras = html::to_document_styled(
        "<html><body><p>a <span class=\"c\">b</span> <span class=\"plain\">c</span></p></body></html>",
        &classes,
    )
    .paras;
    assert!(
        first_text(&paras).emphasis().is_empty(),
        "nothing here is emphasised: {:?}",
        leaning(first_text(&paras))
    );
}

/// An EPUB whose one paragraph leans on a phrase, so the whole path can be
/// walked: parse, passage, wrap, draw.
fn epub_with_emphasis(name: &str) -> PathBuf {
    let dir = common::isolated_home().join("sources").join(name);
    std::fs::create_dir_all(&dir).expect("source dir");
    let path = dir.join(format!("{name}.epub"));
    let file = std::fs::File::create(&path).expect("create epub");
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut put = |name: &str, body: &[u8]| {
        zip.start_file(name, opts).expect("start file");
        zip.write_all(body).expect("write entry");
    };
    put("mimetype", b"application/epub+zip");
    put(
        "META-INF/container.xml",
        br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
              <rootfiles><rootfile full-path="c.opf"
                media-type="application/oebps-package+xml"/></rootfiles>
            </container>"#,
    );
    put(
        "c.opf",
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
             <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>斜体的书</dc:title></metadata>
             <manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest>
             <spine><itemref idref="a"/></spine>
           </package>"#
            .as_bytes(),
    );
    put(
        "a.xhtml",
        r#"<html><body><p>这一段里只有 <em>斜体的这四个字</em> 需要倾斜，其余都是正文，长度足够撑开一整段。</p></body></html>"#
            .as_bytes(),
    );
    zip.finish().expect("finish epub");
    std::fs::canonicalize(&path).unwrap_or(path)
}

#[test]
fn the_emphasised_phrase_reaches_the_screen_in_italics() {
    common::isolated_home();
    let book = Book::load(Some(&epub_with_emphasis("emph-render"))).expect("load epub");
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    app.turn.set_cps(4_000.0);
    let mut terminal = Terminal::new(TestBackend::new(72, 24)).expect("terminal");

    // Opening a book shows the welcome and the plan; the prose arrives when the
    // reader asks for it.
    let settle = |app: &mut App, terminal: &mut Terminal<TestBackend>| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while app.turn.busy() && std::time::Instant::now() < deadline {
            app.on_tick();
            terminal.draw(|frame| app.draw(frame)).expect("draw");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    settle(&mut app, &mut terminal);
    app.on_key(ratatui::crossterm::event::KeyEvent::from(
        ratatui::crossterm::event::KeyCode::Enter,
    ));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        app.on_tick();
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    app.on_tick();
    terminal.draw(|frame| app.draw(frame)).expect("draw");

    let buffer = terminal.backend().buffer();
    let mut italic = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if cell.modifier.contains(Modifier::ITALIC) {
                italic.push_str(cell.symbol());
            }
        }
    }
    assert!(
        italic.contains("斜体的这四个字"),
        "the emphasised phrase should be the italic part of the screen, got {italic:?}"
    );
    assert!(
        !italic.contains("其余都是正文"),
        "and only that phrase: {italic:?}"
    );
}
