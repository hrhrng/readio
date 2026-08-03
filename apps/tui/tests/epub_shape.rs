//! What an EPUB says its shape is, and readio agreeing with it.
//!
//! Three things a reader takes for granted and readio used to get wrong: a
//! table of contents that points inside a file names chapters inside that file,
//! documents marked as not part of the reading order are not read, and a
//! position survives readio changing its mind about where chapters begin.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::{Progress, Store};

/// PNG bytes for a cover, written through the `image` crate so the loader's
/// format check sees a real picture.
fn png_bytes(tag: &str) -> Vec<u8> {
    let dir = common::isolated_home().join("scratch");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(format!("{tag}.png"));
    let mut buffer = image::RgbImage::new(40, 60);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgb([180, 40, 40]);
    }
    buffer.save(&path).expect("write png");
    std::fs::read(&path).expect("read png")
}

/// An EPUB shaped like the ones that made this necessary: one long spine
/// document carrying three chapters, a table of contents pointing at anchors
/// inside it, an appendix marked `linear="no"`, and a declared cover.
fn split_epub(name: &str) -> PathBuf {
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
              <rootfiles><rootfile full-path="OEBPS/content.opf"
                media-type="application/oebps-package+xml"/></rootfiles>
            </container>"#,
    );
    put(
        "OEBPS/content.opf",
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>拆开的书</dc:title><dc:creator>某人</dc:creator>
                <meta name="cover" content="cover-img"/>
              </metadata>
              <manifest>
                <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
                <item id="whole" href="whole.xhtml" media-type="application/xhtml+xml"/>
                <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/>
                <item id="cover-img" href="images/cover.png" media-type="image/png"/>
              </manifest>
              <spine toc="ncx">
                <itemref idref="whole"/>
                <itemref idref="notes" linear="no"/>
              </spine>
            </package>"#
            .as_bytes(),
    );
    put(
        "OEBPS/toc.ncx",
        r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>
              <navPoint id="n1"><navLabel><text>开场</text></navLabel>
                <content src="whole.xhtml"/></navPoint>
              <navPoint id="n2"><navLabel><text>中场</text></navLabel>
                <content src="whole.xhtml#middle"/></navPoint>
              <navPoint id="n3"><navLabel><text>收场</text></navLabel>
                <content src="whole.xhtml#end"/></navPoint>
            </navMap></ncx>"#
            .as_bytes(),
    );
    put(
        "OEBPS/whole.xhtml",
        r#"<html><body>
             <p>开场的第一段，长到足以被读出来。</p>
             <p>开场的第二段，也一样长。</p>
             <h2 id="middle">中场的标题</h2>
             <p>中场的正文，写了一整段。</p>
             <h2 id="end">收场的标题</h2>
             <p>收场的正文，同样值得一读。</p>
           </body></html>"#
            .as_bytes(),
    );
    put(
        "OEBPS/notes.xhtml",
        r#"<html><body><p>版权页与广告，不属于阅读顺序。</p></body></html>"#.as_bytes(),
    );
    put("OEBPS/images/cover.png", &png_bytes(name));
    zip.finish().expect("finish epub");
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn titles(book: &Book) -> Vec<String> {
    book.chapters.iter().map(|c| c.title.clone()).collect()
}

#[test]
fn a_table_of_contents_pointing_inside_a_file_names_chapters_inside_it() {
    let book = Book::load(Some(&split_epub("split-toc"))).expect("load epub");
    assert_eq!(
        titles(&book),
        vec!["开场", "中场", "收场"],
        "three ToC entries in one document are three chapters, named as the ToC names them"
    );
    assert!(
        book.chapters[1].href.ends_with("whole.xhtml#middle"),
        "a chapter cut at an anchor should say which anchor: {}",
        book.chapters[1].href
    );
    assert_eq!(
        book.chapters[1].paras.len(),
        2,
        "the heading and its paragraph belong to the middle chapter"
    );
}

/// The ToC label names the whole document; the heading names each cut. Keeping
/// both apart is what stops "开场" from swallowing the rest of the file.
#[test]
fn the_leading_piece_keeps_the_documents_own_name() {
    let book = Book::load(Some(&split_epub("split-lead"))).expect("load epub");
    assert_eq!(book.chapters[0].title, "开场");
    assert_eq!(book.chapters[0].paras.len(), 2);
    assert!(
        !book.chapters[0].href.contains('#'),
        "the leading piece is the file itself: {}",
        book.chapters[0].href
    );
}

#[test]
fn a_document_marked_not_linear_is_not_read() {
    let book = Book::load(Some(&split_epub("split-linear"))).expect("load epub");
    let text: String = book
        .chapters
        .iter()
        .flat_map(|c| c.paras.iter())
        .map(|p| p.text().to_string())
        .collect();
    assert!(
        !text.contains("广告"),
        "linear=\"no\" is out of the reading order, so its text is not in the book"
    );
    assert_eq!(book.chapters.len(), 3, "only the linear document is read");
}

/// Two chapters from one file cannot both start at line 1: a `Read` call that
/// says `L1-9` twice for the same document is the sort of detail that gives the
/// disguise away.
#[test]
fn line_numbers_continue_through_a_document_that_holds_several_chapters() {
    let book = Book::load(Some(&split_epub("split-lines"))).expect("load epub");
    assert_eq!(book.chapters[0].first_line, 1);
    assert_eq!(
        book.chapters[1].first_line,
        book.chapters[0].first_line + book.chapters[0].line_count() - 1,
        "the second chapter picks up where the first one left off"
    );
    assert!(
        book.chapters[2].first_line > book.chapters[1].first_line,
        "and the third after that: {} vs {}",
        book.chapters[2].first_line,
        book.chapters[1].first_line
    );
    assert!(
        book.line_of(2, 0) > book.line_of(1, 0),
        "later chapters quote later lines"
    );
}

#[test]
fn a_declared_cover_is_found_and_kept() {
    let book = Book::load(Some(&split_epub("split-cover"))).expect("load epub");
    let cover = book.cover.as_ref().expect("cover");
    assert!(
        cover.href.ends_with("images/cover.png"),
        "the cover keeps its place in the archive: {}",
        cover.href
    );
    assert!(cover.file.exists(), "the cover is extracted to disk");
}

/// The migration this release needs: progress stored against thirteen chapters,
/// reopened against seventy-two.
#[test]
fn a_character_offset_outranks_a_stale_chapter_index() {
    let path = split_epub("split-offset");
    let book = Book::load(Some(&path)).expect("load epub");
    let target = book.chars_before(2, 0);
    assert_eq!(
        book.locate(target),
        (2, 0),
        "an offset resolves to the position it was taken from"
    );

    // What an old state file looks like: a position from a coarser cut of the
    // same book, where chapter 0 was the whole document.
    let mut store = Store::ephemeral();
    store.record(
        &book.id,
        Progress {
            title: book.title.clone(),
            path: Some(path.to_string_lossy().into_owned()),
            chapter: 0,
            para: 4,
            chars_read: target as u64,
            sessions: 1,
            updated: 0,
            speech: None,
            marks: Vec::new(),
        },
    );

    let mut app = App::new(Library::ephemeral(), store, Some(book), None);
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("terminal");
    app.on_tick();
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let header = row(&terminal, 0);
    assert!(
        header.contains("3/3"),
        "the reader should land where the offset says, not where the index says: {header}"
    );
}

fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    let mut x = 0u16;
    while x < buffer.area.width {
        let symbol = buffer[(x, y)].symbol();
        out.push_str(symbol);
        x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
    }
    out.trim_end().to_string()
}

/// Guard against the helper drifting from the fixture it documents.
#[test]
fn the_fixture_is_one_document_with_three_sections() {
    let book = Book::load(Some(&split_epub("split-shape"))).expect("load epub");
    let files: Vec<String> = book
        .chapters
        .iter()
        .map(|c| {
            c.href
                .split('#')
                .next()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(files, vec!["whole.xhtml"; 3]);
    assert!(Path::new(&book.path.clone().unwrap()).exists());
}
