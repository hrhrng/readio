//! In-terminal image tests.
//!
//! No fixture files: every image is generated at runtime with the `image`
//! crate and every archive is built with `zip::ZipWriter`, so the tests prove
//! the decode → fit → half-block path end to end without binary blobs in git.

use std::io::Write;
use std::path::{Path, PathBuf};

use image::{ImageFormat, Rgba, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use readio::book::media;
use readio::theme::theme;
use readio::ui::image::{TermImage, fit_in, placeholder};

/// Per-test scratch directory, named after the caller so tests never collide.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("readio-image-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Write a solid-colour PNG and return its path.
fn write_png(dir: &Path, name: &str, w: u32, h: u32, colour: [u8; 4]) -> PathBuf {
    let mut img = RgbaImage::new(w, h);
    for pixel in img.pixels_mut() {
        *pixel = Rgba(colour);
    }
    let path = dir.join(name);
    img.save_with_format(&path, ImageFormat::Png).expect("png");
    path
}

fn png_bytes(w: u32, h: u32, colour: [u8; 4]) -> Vec<u8> {
    let mut img = RgbaImage::new(w, h);
    for pixel in img.pixels_mut() {
        *pixel = Rgba(colour);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .expect("encode png");
    out.into_inner()
}

fn buffer(w: u16, h: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, w, h))
}

fn theme_bg() -> Color {
    theme().bg_base
}

// ── half-block rendering ─────────────────────────────────────────────────────

/// The core of the technique: `▀` with fg = top pixel, bg = bottom pixel. A
/// solid image must therefore produce cells whose fg and bg are both that
/// colour, which is what makes the block look like a filled rectangle.
#[test]
fn a_solid_image_fills_cells_with_half_blocks() {
    let dir = scratch("solid");
    let red = [220u8, 40, 40, 255];
    let path = write_png(&dir, "red.png", 2, 4, red);

    let img = TermImage::open(&path, 2, 2).expect("decode");
    assert_eq!(
        (img.fit().cols, img.fit().rows),
        (2, 2),
        "2×4 px is exactly 2×2 cells at two pixels per row"
    );

    let mut buf = buffer(4, 4);
    let rows = img.render(Rect::new(0, 0, 2, 2), &mut buf);
    assert_eq!(rows, 2, "both cell rows should be written");

    let expected = Color::Rgb(red[0], red[1], red[2]);
    for y in 0..2u16 {
        for x in 0..2u16 {
            let cell = &buf[(x, y)];
            assert_eq!(cell.symbol(), "▀", "cell ({x},{y}) should be a half block");
            assert_eq!(
                (cell.fg, cell.bg),
                (expected, expected),
                "cell ({x},{y}) should carry the image colour in both halves"
            );
        }
    }
    // Outside the fit nothing may be touched: images must not bleed into text.
    assert_eq!(
        buf[(2, 0)].symbol(),
        " ",
        "the column past the image must stay untouched"
    );
}

/// A cell is twice as tall as it is wide, so the pixel grid is `cols × rows*2`.
/// If that factor were dropped, every image would render at half height.
#[test]
fn aspect_ratio_survives_the_cell_geometry() {
    let dir = scratch("aspect");
    let wide = write_png(&dir, "wide.png", 400, 100, [10, 200, 10, 255]);
    let tall = write_png(&dir, "tall.png", 100, 400, [10, 10, 200, 255]);
    let square = write_png(&dir, "square.png", 200, 200, [200, 200, 10, 255]);

    let (max_cols, max_rows) = (40u16, 40u16);
    let wide_fit = TermImage::open(&wide, max_cols, max_rows)
        .expect("wide")
        .fit();
    let tall_fit = TermImage::open(&tall, max_cols, max_rows)
        .expect("tall")
        .fit();
    let square_fit = TermImage::open(&square, max_cols, max_rows)
        .expect("square")
        .fit();

    assert!(
        wide_fit.cols > wide_fit.rows,
        "a 4:1 image should be wider than it is tall in cells: {wide_fit:?}"
    );
    assert!(
        tall_fit.rows > tall_fit.cols,
        "a 1:4 image should be taller than it is wide in cells: {tall_fit:?}"
    );
    assert_eq!(
        (square_fit.cols, square_fit.rows),
        (40, 20),
        "a square image must halve its rows, or it renders stretched: {square_fit:?}"
    );

    for (label, fit) in [
        ("wide", wide_fit),
        ("tall", tall_fit),
        ("square", square_fit),
    ] {
        assert!(
            fit.cols <= max_cols && fit.rows <= max_rows,
            "{label} exceeded the requested maxima: {fit:?}"
        );
    }

    // The wide image's pixel ratio (4:1) becomes 8:1 in cells, because a cell
    // row is worth two pixels.
    assert_eq!(
        (wide_fit.cols, wide_fit.rows),
        (40, 5),
        "40 columns of a 4:1 image is 10 pixel rows, i.e. 5 cell rows"
    );

    // Layout happens before decoding, so the cheap prediction has to match.
    for (label, w, h, fit) in [
        ("wide", 400, 100, wide_fit),
        ("tall", 100, 400, tall_fit),
        ("square", 200, 200, square_fit),
    ] {
        assert_eq!(
            fit_in(w, h, max_cols, max_rows),
            fit,
            "fit_in disagrees with the decoded {label} image, so reserved space would be wrong"
        );
    }
}

/// Alpha must be composited against the reading background. Left alone, the
/// decoder hands back black under transparent pixels and the reader sees a
/// black box where the illustration should have blended in.
#[test]
fn transparency_composites_over_the_theme_background() {
    let dir = scratch("alpha");
    let clear = write_png(&dir, "clear.png", 4, 4, [255, 0, 0, 0]);

    let img = TermImage::open(&clear, 4, 2).expect("decode");
    let mut buf = buffer(4, 2);
    img.render(Rect::new(0, 0, 4, 2), &mut buf);

    let cell = &buf[(0, 0)];
    assert_eq!(
        cell.fg,
        theme_bg(),
        "a fully transparent pixel should read as the background, not black"
    );
    assert_ne!(
        cell.fg,
        Color::Rgb(0, 0, 0),
        "black here would mean alpha was ignored"
    );

    // Half alpha lands between the two, and specifically is neither endpoint.
    let half = write_png(&dir, "half.png", 4, 4, [255, 255, 255, 128]);
    let img = TermImage::open(&half, 4, 2).expect("decode");
    let mut buf = buffer(4, 2);
    img.render(Rect::new(0, 0, 4, 2), &mut buf);
    let Color::Rgb(r, _, _) = buf[(0, 0)].fg else {
        panic!("expected a truecolor cell, got {:?}", buf[(0, 0)].fg);
    };
    assert!(
        (100..=200).contains(&r),
        "50% white over a dark background should land mid-range, got {r}"
    );
}

/// An odd pixel height means the bottom cell row has a top pixel but no bottom
/// one; the missing half is filled with background rather than read out of
/// bounds.
#[test]
fn odd_pixel_heights_do_not_panic_and_fill_the_last_half() {
    let dir = scratch("odd");
    let path = write_png(&dir, "odd.png", 3, 3, [90, 160, 240, 255]);

    let img = TermImage::open(&path, 3, 2).expect("decode");
    let fit = img.fit();
    assert_eq!(
        (fit.cols, fit.rows),
        (3, 2),
        "3 px of height needs two cell rows: {fit:?}"
    );

    let mut buf = buffer(3, 2);
    let rows = img.render(Rect::new(0, 0, 3, 2), &mut buf);
    assert_eq!(rows, 2);
    let last = &buf[(0, 1)];
    assert_eq!(last.symbol(), "▀");
    assert_eq!(
        last.bg,
        theme_bg(),
        "the missing bottom pixel should be background, not a repeat or black"
    );
    assert_ne!(
        last.fg,
        theme_bg(),
        "the top half of the last row is still image"
    );
}

// ── clipping and degenerate areas ────────────────────────────────────────────

#[test]
fn a_small_area_clips_instead_of_panicking() {
    let dir = scratch("clip");
    let path = write_png(&dir, "big.png", 40, 40, [200, 60, 120, 255]);
    let img = TermImage::open(&path, 20, 20).expect("decode");
    assert!(img.fit().cols > 3 && img.fit().rows > 1);

    let mut buf = buffer(20, 20);
    let rows = img.render(Rect::new(0, 0, 3, 1), &mut buf);
    assert_eq!(rows, 1, "only the rows that fit may be reported");
    assert_eq!(buf[(0, 0)].symbol(), "▀");
    assert_eq!(
        buf[(3, 0)].symbol(),
        " ",
        "clipping must not write past the area's width"
    );
    assert_eq!(
        buf[(0, 1)].symbol(),
        " ",
        "clipping must not write past the area's height"
    );
}

#[test]
fn a_zero_size_area_writes_nothing() {
    let dir = scratch("zero");
    let path = write_png(&dir, "any.png", 8, 8, [1, 2, 3, 255]);
    let img = TermImage::open(&path, 8, 4).expect("decode");

    let mut buf = buffer(8, 4);
    let before = buf.clone();
    assert_eq!(img.render(Rect::new(0, 0, 0, 4), &mut buf), 0);
    assert_eq!(img.render(Rect::new(0, 0, 8, 0), &mut buf), 0);
    assert_eq!(
        buf, before,
        "a zero-size area must leave the buffer exactly as it was"
    );

    // Asking for zero cells is legal and simply yields an empty fit.
    let none = TermImage::open(&path, 0, 4).expect("zero-width fit is not an error");
    assert!(none.fit().is_empty(), "{:?} should be empty", none.fit());
    assert_eq!(none.render(Rect::new(0, 0, 8, 4), &mut buf), 0);
}

/// An offset area must be honoured, or an image drawn in a column would land on
/// top of the text beside it.
#[test]
fn rendering_respects_the_area_origin() {
    let dir = scratch("origin");
    let path = write_png(&dir, "spot.png", 2, 2, [30, 220, 30, 255]);
    let img = TermImage::open(&path, 2, 1).expect("decode");

    let mut buf = buffer(6, 4);
    img.render(Rect::new(3, 2, 2, 1), &mut buf);
    assert_eq!(buf[(3, 2)].symbol(), "▀");
    assert_eq!(
        buf[(0, 0)].symbol(),
        " ",
        "nothing above-left may be touched"
    );
}

// ── failures and the placeholder ─────────────────────────────────────────────

#[test]
fn corrupt_files_error_and_the_placeholder_still_draws() {
    let dir = scratch("corrupt");

    // A PNG cut off mid-stream: the header parses, the data does not.
    let full = png_bytes(64, 64, [10, 10, 10, 255]);
    let truncated = dir.join("truncated.png");
    std::fs::write(&truncated, &full[..full.len() / 3]).expect("write truncated");
    assert!(
        TermImage::open(&truncated, 20, 10).is_err(),
        "a truncated PNG must be an error, not a panic and not a blank image"
    );

    let garbage = dir.join("garbage.png");
    std::fs::write(&garbage, b"this is not an image at all").expect("write garbage");
    assert!(TermImage::open(&garbage, 20, 10).is_err());

    let empty = dir.join("empty.png");
    std::fs::write(&empty, b"").expect("write empty");
    assert!(TermImage::open(&empty, 20, 10).is_err());

    assert!(
        TermImage::open(&dir.join("missing.png"), 20, 10).is_err(),
        "a missing file must error rather than being silently skipped"
    );

    // The reader still needs to see that something was there.
    let mut buf = buffer(24, 5);
    placeholder(Rect::new(0, 0, 20, 5), &mut buf, "封面.png");
    assert_eq!(buf[(0, 0)].symbol(), "┌", "the frame should have corners");
    assert_eq!(buf[(19, 4)].symbol(), "┘");
    let drawn = (0..5)
        .flat_map(|y| (0..20).map(move |x| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
        .count();
    assert!(
        drawn > 20,
        "the placeholder should be visibly framed, only {drawn} cells drawn"
    );
    let label_row: String = (0..20).map(|x| buf[(x, 2)].symbol()).collect();
    assert!(
        label_row.contains("封面"),
        "the label should name the file: {label_row:?}"
    );
    assert_eq!(
        buf[(20, 0)].symbol(),
        " ",
        "the placeholder must stay inside its area"
    );
}

#[test]
fn a_tiny_area_gets_no_placeholder_rather_than_a_broken_one() {
    let mut buf = buffer(4, 4);
    let before = buf.clone();
    placeholder(Rect::new(0, 0, 1, 1), &mut buf, "x.png");
    assert_eq!(
        buf, before,
        "one cell cannot hold a frame, so nothing should be drawn"
    );
}

// ── href resolution ──────────────────────────────────────────────────────────

#[test]
fn img_srcs_resolve_to_archive_keys() {
    assert_eq!(
        media::resolve_href("OEBPS/text/ch1.xhtml", "../images/a.png"),
        "oebps/images/a.png"
    );
    assert_eq!(
        media::resolve_href("OEBPS/text/ch1.xhtml", "./a.png"),
        "oebps/text/a.png"
    );
    assert_eq!(
        media::resolve_href("OEBPS/text/ch1.xhtml", "images/a%20b.png"),
        "oebps/text/images/a b.png",
        "percent escapes must decode, or the zip lookup misses"
    );
    assert_eq!(
        media::resolve_href("OEBPS/text/ch1.xhtml", "/images/a.png"),
        "images/a.png",
        "an absolute href is rooted at the archive, not at the document"
    );
}

// ── EPUB extraction ─────────────────────────────────────────────────────────

/// Build an archive with one good PNG, one SVG, one oversized entry, one
/// mislabelled entry and one zip-slip attempt.
fn build_archive(path: &Path, png: &[u8], oversized: &[u8]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut put = |name: &str, bytes: &[u8]| {
        zip.start_file(name, options).expect("start file");
        zip.write_all(bytes).expect("write entry");
    };

    put("OEBPS/images/good.png", png);
    put("OEBPS/images/vector.svg", br#"<svg xmlns="http://x"/>"#);
    put("OEBPS/images/huge.png", oversized);
    // Real EPUBs do this: an XHTML file wearing an image extension.
    put(
        "OEBPS/images/liar.png",
        b"<html><body>not an image</body></html>",
    );
    put("OEBPS/text/ch1.xhtml", b"<html/>");
    put("../../evil.png", png);
    zip.finish().expect("finish zip");
}

#[test]
fn extraction_keeps_images_and_refuses_everything_else() {
    let dir = scratch("extract");
    let epub = dir.join("book.epub");
    let dest = dir.join("cache");
    let png = png_bytes(6, 6, [77, 88, 99, 255]);
    // Just over the 20 MB ceiling, wearing a valid PNG header so only the size
    // check can reject it.
    let mut oversized = png.clone();
    oversized.resize(21 * 1024 * 1024, 0);
    build_archive(&epub, &png, &oversized);

    let map = media::extract_epub_images(&epub, &dest).expect("extract");

    let key = "oebps/images/good.png";
    let extracted = map.get(key).expect("the good PNG should be extracted");
    assert!(
        extracted.exists(),
        "{} should be on disk",
        extracted.display()
    );
    assert_eq!(
        std::fs::read(extracted).expect("read back"),
        png,
        "the extracted bytes must be the archive's bytes"
    );
    assert!(
        extracted.starts_with(&dest),
        "{} escaped the cache dir",
        extracted.display()
    );

    assert!(
        !map.contains_key("oebps/images/vector.svg"),
        "SVG is out of scope: {map:?}"
    );
    assert!(
        !map.contains_key("oebps/images/huge.png"),
        "an oversized entry must be skipped: {map:?}"
    );
    assert!(
        !map.contains_key("oebps/images/liar.png"),
        "magic bytes must override the extension: {map:?}"
    );
    assert!(
        !map.contains_key("oebps/text/ch1.xhtml"),
        "documents are not images: {map:?}"
    );
    assert_eq!(map.len(), 1, "exactly one entry should survive: {map:?}");

    // Zip slip: neither the map nor the filesystem may show the escape.
    assert!(
        map.keys().all(|k| !k.contains("evil")),
        "a `../../` entry must be refused: {map:?}"
    );
    assert!(
        !dir.join("evil.png").exists() && !dest.join("evil.png").exists(),
        "nothing may be written outside the destination"
    );

    // The href a chapter would use finds the extracted file.
    let href = media::resolve_href("OEBPS/text/ch1.xhtml", "../images/good.png");
    assert_eq!(href, key);
    assert!(
        map.contains_key(&href),
        "resolve_href must agree with the keys"
    );

    // And the extracted file actually decodes and renders.
    let img = TermImage::open(map.get(key).expect("path"), 6, 3).expect("decode extracted");
    let mut buf = buffer(6, 3);
    assert!(
        img.render(Rect::new(0, 0, 6, 3), &mut buf) > 0,
        "the extracted image should draw"
    );
}

/// Reopening a book must not rewrite the cache: the mtime is left alone when
/// the cached file already has the same size.
#[test]
fn extraction_is_idempotent() {
    let dir = scratch("idempotent");
    let epub = dir.join("book.epub");
    let dest = dir.join("cache");
    let png = png_bytes(4, 4, [1, 2, 3, 255]);
    build_archive(&epub, &png, b"small enough");

    let first = media::extract_epub_images(&epub, &dest).expect("first pass");
    let path = first.get("oebps/images/good.png").expect("png").clone();
    let stamp = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .expect("mtime");

    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = media::extract_epub_images(&epub, &dest).expect("second pass");

    assert_eq!(
        first, second,
        "the same archive must yield the same mapping"
    );
    let after = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .expect("mtime");
    assert_eq!(
        stamp, after,
        "an unchanged cached image must not be rewritten"
    );
}

#[test]
fn a_file_that_is_not_a_zip_is_an_error() {
    let dir = scratch("notzip");
    let fake = dir.join("book.epub");
    std::fs::write(&fake, b"plain text pretending to be an EPUB").expect("write");
    assert!(
        media::extract_epub_images(&fake, &dir.join("cache")).is_err(),
        "an unreadable archive is the one case that should surface as an error"
    );
}
