//! Illustrations, end to end: a book with a picture in it must put coloured
//! half-blocks on the screen, not a row of question marks.
//!
//! The pictures are generated here rather than committed, so the test proves
//! the whole path — parse, extract, resolve, decode, draw — with no fixtures to
//! go stale.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use readio::app::App;
use readio::book::{Book, Para};
use readio::library::Library;
use readio::store::Store;

/// Write a PNG of a solid colour.
fn write_png(path: &Path, width: u32, height: u32, rgb: [u8; 3]) {
    let mut buffer = image::RgbImage::new(width, height);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgb(rgb);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("image dir");
    }
    buffer.save(path).expect("write png");
}

/// PNG bytes, written under a name unique to the caller: the tests run in
/// parallel against one temp home, and two of them racing on the same scratch
/// file is exactly how this test suite learns to flake.
fn png_bytes(tag: &str, width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let dir = common::isolated_home().join("scratch");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(format!("{tag}-{width}x{height}.png"));
    write_png(&path, width, height, rgb);
    std::fs::read(&path).expect("read png")
}

/// A minimal EPUB with one chapter that carries one illustration.
fn epub_with_image(name: &str) -> PathBuf {
    let dir = common::isolated_home().join("sources").join(name);
    std::fs::create_dir_all(&dir).expect("source dir");
    let path = dir.join(format!("{name}.epub"));
    let file = std::fs::File::create(&path).expect("create epub");
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let put = |zip: &mut zip::ZipWriter<std::fs::File>, name: &str, body: &[u8]| {
        zip.start_file(name, opts).expect("start file");
        zip.write_all(body).expect("write entry");
    };

    put(
        &mut zip,
        "META-INF/container.xml",
        br#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
    );
    put(
        &mut zip,
        "OEBPS/content.opf",
        r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>带插图的书</dc:title>
    <dc:creator>某人</dc:creator>
  </metadata>
  <manifest>
    <item id="c1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="fig" href="images/fig.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
            .as_bytes(),
    );
    put(
        &mut zip,
        "OEBPS/text/ch1.xhtml",
        r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
  <h2>第一章 一张图</h2>
  <p>下面这张图是这一章的全部论点，文字只是它的注脚。它需要足够长，才能让阅读这一轮真的花掉一点时间。</p>
  <p><img src="../images/fig.png" alt="一张纯色的示意图"/></p>
  <p>图后面还有一段文字，用来确认插图没有把后面的内容吃掉。</p>
</body></html>"#
            .as_bytes(),
    );
    put(
        &mut zip,
        "OEBPS/images/fig.png",
        &png_bytes(name, 64, 32, [200, 60, 60]),
    );
    zip.finish().expect("finish epub");
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// The screen as text. A double-width glyph leaves its second cell untouched
/// in the test backend, so walk each row by display width the way a real
/// terminal does — otherwise stale characters land between the real ones.
fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            row.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Cells that were painted as image pixels: a half block with a real colour.
fn pixel_cells(terminal: &Terminal<TestBackend>) -> usize {
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "▀")
        .count()
}

fn read_until(app: &mut App, terminal: &mut Terminal<TestBackend>, frames: usize) {
    for _ in 0..frames {
        app.on_tick();
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
}

/// Pump until the turn machine goes quiet, so a keypress lands on an idle app.
fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        read_until(app, terminal, 1);
    }
}

fn press_enter(app: &mut App) {
    app.on_key(ratatui::crossterm::event::KeyEvent::from(
        ratatui::crossterm::event::KeyCode::Enter,
    ));
}

#[test]
fn an_epub_illustration_becomes_a_paragraph_with_a_real_file() {
    common::isolated_home();
    let path = epub_with_image("figures");
    let book = Book::load(Some(&path)).expect("load epub");

    let images: Vec<&Para> = book
        .chapters
        .iter()
        .flat_map(|c| c.paras.iter())
        .filter(|p| p.is_image())
        .collect();
    assert_eq!(images.len(), 1, "the chapter has exactly one illustration");

    let Para::Image { src, alt } = images[0] else {
        unreachable!("filtered above");
    };
    assert!(
        src.is_file(),
        "the image should be extracted to a real file, got {}",
        src.display()
    );
    assert_eq!(alt, "一张纯色的示意图", "the caption should survive");
    assert!(
        readio::ui::image::dimensions(src).expect("dimensions") == (64, 32),
        "the extracted bytes should still decode"
    );

    // Text around the picture is untouched.
    let paras = &book.chapters[0].paras;
    assert!(
        paras.iter().any(|p| p.text().contains("注脚")),
        "the paragraph before the image should still be there"
    );
    assert!(
        paras.iter().any(|p| p.text().contains("吃掉")),
        "the paragraph after the image should still be there"
    );
}

#[test]
fn the_picture_is_actually_painted_on_the_screen() {
    common::isolated_home();
    let path = epub_with_image("painted");
    let book = Book::load(Some(&path)).expect("load epub");

    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).expect("terminal");

    // Fast enough to get through the passage inside the frame budget below.
    app.turn.set_cps(4_000.0);
    settle(&mut app, &mut terminal);
    assert_eq!(
        pixel_cells(&terminal),
        0,
        "nothing is drawn before the reading turn reaches the image"
    );

    // Enter starts reading; the turn contains the illustration.
    press_enter(&mut app);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while pixel_cells(&terminal) == 0 && std::time::Instant::now() < deadline {
        read_until(&mut app, &mut terminal, 1);
    }

    let painted = pixel_cells(&terminal);
    assert!(
        painted > 40,
        "the illustration should cover a block of cells, got {painted}"
    );

    // Those cells carry two distinct colours: the picture's red on top, and
    // whatever is below it. A monochrome result would mean we drew a box.
    let buffer = terminal.backend().buffer();
    let coloured = buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "▀")
        .filter(|cell| {
            matches!(cell.fg, ratatui::style::Color::Rgb(r, g, b) if r > g && r > b && r > 100)
        })
        .count();
    assert!(
        coloured > 40,
        "the pixels should carry the image's red, got {coloured} of {painted}"
    );

    // The caption is on screen too, above the pixels.
    let text = screen_text(&terminal);
    assert!(
        text.contains("插图") && text.contains("64×32"),
        "the caption should name the picture and its size:\n{text}"
    );
}

#[test]
fn a_markdown_image_is_resolved_against_the_book() {
    common::isolated_home();
    let dir = common::isolated_home().join("sources").join("md-images");
    std::fs::create_dir_all(&dir).expect("dir");
    write_png(&dir.join("fig.png"), 40, 20, [60, 160, 90]);
    let path = dir.join("notes.md");
    std::fs::write(
        &path,
        "# 有图的笔记\n\n第一段，正常的文字，长到足够被当成一段内容读出来。\n\n![绿色方块](fig.png)\n\n![不存在的图](missing.png)\n\n最后一段。\n",
    )
    .expect("write md");

    let book = Book::load(Some(&path)).expect("load markdown");
    let paras = &book.chapters[0].paras;

    let images: Vec<&Para> = paras.iter().filter(|p| p.is_image()).collect();
    assert_eq!(images.len(), 1, "only the image that exists is an image");
    let Para::Image { src, alt } = images[0] else {
        unreachable!("filtered above");
    };
    assert!(src.is_absolute(), "the path should be resolved: {src:?}");
    assert!(src.is_file(), "and point at the file next to the book");
    assert_eq!(alt, "绿色方块");

    assert!(
        paras.iter().any(|p| p.text() == "不存在的图"),
        "a missing image degrades to its caption rather than vanishing:\n{paras:#?}"
    );
}

#[test]
fn images_can_be_turned_off_in_the_config() {
    common::isolated_home();
    let path = epub_with_image("switched-off");
    let book = Book::load(Some(&path)).expect("load epub");

    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).expect("terminal");
    app.turn.images = false;
    app.turn.set_cps(4_000.0);
    settle(&mut app, &mut terminal);

    press_enter(&mut app);
    settle(&mut app, &mut terminal);

    assert_eq!(
        pixel_cells(&terminal),
        0,
        "with images off, nothing should be painted"
    );
    let text = screen_text(&terminal);
    assert!(
        text.contains("示意图"),
        "the caption should stand in for the picture:\n{text}"
    );
}
