//! Image geometry, placeholders, and the former half-block renderer.
//!
//! Production scrollback rendering uses Kitty, iTerm2, or Sixel through
//! `ratatui-image`. [`TermImage`] remains as an isolated, deterministic
//! renderer for tests and embedders that explicitly call it; readio itself no
//! longer selects it. Its upper-half block `▀` paints the top half of
//! the cell and its background the bottom half, so one cell carries two
//! vertically stacked pixels.

use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use image::RgbImage;
use image::imageops::FilterType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::theme;
use crate::wrap::{display_width, truncate};

/// The glyph whose foreground is the top pixel and background the bottom one.
const HALF_BLOCK: &str = "▀";

/// A cell is about twice as tall as it is wide, so a cell holding two stacked
/// pixels is square: one pixel per cell column, two per cell row.
const PIXELS_PER_ROW: u32 = 2;

/// Largest file we are willing to hand to the decoder.
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Largest side we are willing to decode, in pixels.
const MAX_SIDE: u32 = 8_000;

/// Geometry an image will occupy, in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    pub cols: u16,
    pub rows: u16,
}

impl Fit {
    /// Whether this geometry would draw nothing at all.
    pub fn is_empty(&self) -> bool {
        self.cols == 0 || self.rows == 0
    }
}

/// Legacy environment-only protocol detection, retained for API compatibility.
/// Production uses `ratatui_image::picker::Picker`, which actively queries the
/// terminal and provides its real cell dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Kitty graphics protocol — also spoken by WezTerm and Ghostty.
    Kitty,
    /// iTerm2 inline-images protocol.
    Iterm2,
    /// Unicode half blocks with truecolor attributes. The universal fallback,
    /// and the only variant with an implementation behind it.
    HalfBlock,
}

/// Guess what the host terminal can draw, from the environment alone.
///
/// Nothing is queried over the wire; readio's production path does not call
/// this helper.
pub fn detect_protocol() -> Protocol {
    let term = std::env::var("TERM").unwrap_or_default();
    if term.contains("kitty") || std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return Protocol::Kitty;
    }
    match std::env::var("TERM_PROGRAM").unwrap_or_default().as_str() {
        // WezTerm and Ghostty implement Kitty's protocol, not iTerm2's.
        "WezTerm" | "ghostty" | "Ghostty" => Protocol::Kitty,
        "iTerm.app" => Protocol::Iterm2,
        _ => Protocol::HalfBlock,
    }
}

/// Decoded, downscaled image ready to draw.
///
/// The expensive work — reading, decoding, resampling, alpha compositing —
/// happens once in [`TermImage::open`], so a cached `TermImage` can be redrawn
/// every frame for the cost of writing cells. The stored pixels are already at
/// the exact grid the half-block renderer wants: `cols` wide and `rows * 2`
/// tall at most, fully opaque.
#[derive(Debug, Clone)]
pub struct TermImage {
    /// Opaque RGB pixels, `fit.cols` wide, at most `fit.rows * 2` tall.
    pixels: RgbImage,
    fit: Fit,
}

impl TermImage {
    /// Decode `path` and downscale it to fit within `max_cols` × `max_rows`
    /// cells, preserving aspect ratio.
    ///
    /// Errors rather than blocking on absurd input: files over 50 MB and images
    /// whose decoded header claims a side over 8000 px are refused, because a
    /// reader that stalls for ten seconds on one illustration is worse than a
    /// reader that shows a placeholder.
    pub fn open(path: &Path, max_cols: u16, max_rows: u16) -> Result<Self> {
        let meta =
            std::fs::metadata(path).with_context(|| format!("读不到图片 {}", path.display()))?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(anyhow!(
                "图片 {} 有 {} MB，太大了，跳过",
                path.display(),
                meta.len() / (1024 * 1024)
            ));
        }

        let bytes =
            std::fs::read(path).with_context(|| format!("读不到图片 {}", path.display()))?;
        let reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .with_context(|| format!("认不出图片格式 {}", path.display()))?;
        // Reading the header is cheap and is the only chance to refuse a
        // 30000 px image before the decoder allocates a buffer for it.
        if let Ok((w, h)) = reader.into_dimensions()
            && (w > MAX_SIDE || h > MAX_SIDE)
        {
            return Err(anyhow!("图片 {} 是 {w}×{h}，尺寸超出上限", path.display()));
        }

        let decoded = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .with_context(|| format!("认不出图片格式 {}", path.display()))?
            .decode()
            .with_context(|| format!("图片 {} 解码失败", path.display()))?;

        Self::from_dynamic(&decoded, max_cols, max_rows)
    }

    /// Downscale an already-decoded image. Split out from [`TermImage::open`]
    /// so callers that already hold pixels — a test, or a future cover-art
    /// path that decodes straight out of the zip — can reuse the mapping.
    pub fn from_dynamic(
        source: &image::DynamicImage,
        max_cols: u16,
        max_rows: u16,
    ) -> Result<Self> {
        let (src_w, src_h) = (source.width(), source.height());
        if src_w == 0 || src_h == 0 {
            return Err(anyhow!("图片没有像素"));
        }
        if src_w > MAX_SIDE || src_h > MAX_SIDE {
            return Err(anyhow!("图片是 {src_w}×{src_h}，尺寸超出上限"));
        }

        let (target_w, target_h) = pixel_grid(src_w, src_h, max_cols, max_rows);
        if target_w == 0 || target_h == 0 {
            // Nothing to draw, but not an error: a two-column sidebar is a
            // legitimate place to ask for an image and get none.
            return Ok(Self {
                pixels: RgbImage::new(0, 0),
                fit: Fit { cols: 0, rows: 0 },
            });
        }
        let fit = Fit {
            cols: target_w as u16,
            // Half a cell still costs a whole cell; `render` fills the unused
            // bottom pixel of an odd grid with background.
            rows: target_h.div_ceil(PIXELS_PER_ROW) as u16,
        };

        let scaled = source
            .resize_exact(target_w, target_h, FilterType::Triangle)
            .to_rgba8();

        // Composite onto the reading background: a transparent PNG left as-is
        // would arrive with black under the alpha and draw as a black box.
        let (br, bg, bb) = rgb_of(theme().bg_base);
        let mut pixels = RgbImage::new(target_w, target_h);
        for (x, y, px) in scaled.enumerate_pixels() {
            let [r, g, b, a] = px.0;
            pixels.put_pixel(
                x,
                y,
                image::Rgb([blend(r, br, a), blend(g, bg, a), blend(b, bb, a)]),
            );
        }

        Ok(Self { pixels, fit })
    }

    /// Geometry this image will occupy.
    pub fn fit(&self) -> Fit {
        self.fit
    }

    /// Draw into `area`, top-left aligned. Returns rows actually written.
    ///
    /// Clips instead of complaining: an area narrower or shorter than the
    /// image simply shows the top-left of it, and a zero-size area writes
    /// nothing. Frames are resized under us constantly, so a mismatch is
    /// normal, not exceptional.
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> u16 {
        self.render_clipped(area, buf, 0)
    }

    /// Draw with the first `skip_rows` cell rows left out, which is what a
    /// scrollback needs when an illustration is half-scrolled off the top.
    pub fn render_clipped(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) -> u16 {
        let (px_w, px_h) = self.pixels.dimensions();
        if area.width == 0 || area.height == 0 || px_w == 0 || px_h == 0 {
            return 0;
        }
        let cols = self.fit.cols.min(area.width);
        let rows = self.fit.rows.saturating_sub(skip_rows).min(area.height);
        let mut written = 0u16;

        for row in 0..rows {
            let top_y = u32::from(row + skip_rows) * PIXELS_PER_ROW;
            if top_y >= px_h {
                break;
            }
            for col in 0..cols {
                let x = u32::from(col);
                if x >= px_w {
                    break;
                }
                let top = self.pixels.get_pixel(x, top_y).0;
                // An odd pixel height leaves the final row with only a top
                // pixel; painting the background under it keeps the image from
                // gaining a smeared extra half-row.
                let bottom = if top_y + 1 < px_h {
                    self.pixels.get_pixel(x, top_y + 1).0
                } else {
                    let (r, g, b) = rgb_of(theme().bg_base);
                    [r, g, b]
                };

                let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else {
                    continue;
                };
                cell.set_symbol(HALF_BLOCK);
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                );
            }
            written += 1;
        }
        written
    }
}

/// Pixel grid an image of `src_w` × `src_h` should be resampled to, given the
/// cell maxima. One pixel per column, two per row.
///
/// Two pixels per cell row is what keeps the picture from looking stretched: a
/// cell is about twice as tall as it is wide, so a square image wants half as
/// many rows as columns. The scale factor is taken from whichever axis binds
/// first and applied to both, so the ratio survives. The height is left exact
/// rather than rounded up to an even number of pixels — a 3 px tall thumbnail
/// stays 3 px tall and occupies one and a half cells.
///
/// Returns `(0, 0)` for a degenerate request; the caller treats that as "draw
/// nothing", not as an error.
fn pixel_grid(src_w: u32, src_h: u32, max_cols: u16, max_rows: u16) -> (u32, u32) {
    if src_w == 0 || src_h == 0 || max_cols == 0 || max_rows == 0 {
        return (0, 0);
    }
    // 64-bit throughout: the cross-multiplication below would overflow u32 for
    // a wide image against a large terminal.
    let (src_w, src_h) = (u64::from(src_w), u64::from(src_h));
    let max_px_w = u64::from(max_cols);
    let max_px_h = u64::from(max_rows) * u64::from(PIXELS_PER_ROW);

    // Compare src_w/max_px_w against src_h/max_px_h without floats, then keep
    // whichever axis binds first and scale the other by the same factor.
    let (px_w, px_h) = if src_w * max_px_h >= src_h * max_px_w {
        let h = (src_h * max_px_w).div_ceil(src_w).max(1);
        (max_px_w, h.min(max_px_h))
    } else {
        let w = (src_w * max_px_h).div_ceil(src_h).max(1);
        (w.min(max_px_w), max_px_h)
    };
    (
        px_w.clamp(1, max_px_w) as u32,
        px_h.clamp(1, max_px_h) as u32,
    )
}

/// Pixel size of an image without decoding it, so layout can reserve rows.
pub fn dimensions(path: &Path) -> Result<(u32, u32)> {
    image::image_dimensions(path).with_context(|| format!("认不出图片 {}", path.display()))
}

/// Cells an image of `src_w` × `src_h` pixels would need inside the given
/// maxima.
pub fn fit_in(src_w: u32, src_h: u32, max_cols: u16, max_rows: u16) -> Fit {
    let (px_w, px_h) = pixel_grid(src_w, src_h, max_cols, max_rows);
    Fit {
        cols: px_w as u16,
        rows: px_h.div_ceil(PIXELS_PER_ROW) as u16,
    }
}

/// Draw a dim framed box carrying `label`, for images that cannot be decoded.
///
/// The frame is what tells a reader "there was a picture here" instead of
/// silently dropping content; the label says which file failed.
pub fn placeholder(area: Rect, buf: &mut Buffer, label: &str) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let dim = Style::default().fg(theme().text_faint).bg(theme().bg_base);
    let last_x = area.x + area.width - 1;
    let last_y = area.y + area.height - 1;

    let mut put = |x: u16, y: u16, symbol: &str| {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_symbol(symbol);
            cell.set_style(dim);
        }
    };

    for x in area.x..=last_x {
        put(x, area.y, "─");
        put(x, last_y, "─");
    }
    for y in area.y..=last_y {
        put(area.x, y, "│");
        put(last_x, y, "│");
    }
    put(area.x, area.y, "┌");
    put(last_x, area.y, "┐");
    put(area.x, last_y, "└");
    put(last_x, last_y, "┘");

    // One line of label, centred vertically, inset by the frame and a space.
    let inner_width = usize::from(area.width.saturating_sub(4));
    if inner_width == 0 {
        return;
    }
    let text = truncate(label, inner_width);
    let mut x = area.x + 2;
    let y = area.y + area.height / 2;
    for grapheme in text.graphemes(true) {
        let width = display_width(grapheme).max(1) as u16;
        if x + width > last_x {
            break;
        }
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_symbol(grapheme);
            cell.set_style(dim);
        }
        // A double-width glyph owns the cell to its right; ratatui expects that
        // cell to hold an empty symbol, otherwise whatever was there before is
        // still sent to the terminal and the label comes out spaced apart.
        for extra in 1..width {
            if let Some(cell) = buf.cell_mut((x + extra, y)) {
                cell.reset();
                cell.set_symbol("");
                cell.set_style(dim);
            }
        }
        x += width;
    }
}

/// Source-over compositing of one channel against an opaque backdrop.
fn blend(fg: u8, bg: u8, alpha: u8) -> u8 {
    let a = u32::from(alpha);
    let mixed = u32::from(fg) * a + u32::from(bg) * (255 - a);
    // Round rather than truncate, so a 50 % white over black lands on 128.
    ((mixed + 127) / 255).min(255) as u8
}

/// Channel triple for a theme colour. The palette is all `Color::Rgb`, so the
/// named and indexed variants only need a defined answer, not a good one.
fn rgb_of(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_image_gets_half_as_many_rows_as_columns() {
        let fit = fit_in(100, 100, 40, 40);
        assert_eq!(
            (fit.cols, fit.rows),
            (40, 20),
            "a cell is two pixels tall, so a square image must halve its rows"
        );
    }

    #[test]
    fn fit_never_exceeds_the_requested_maxima() {
        for (w, h) in [(1, 1), (3, 7), (1920, 1080), (7999, 3), (3, 7999)] {
            for (cols, rows) in [(1u16, 1u16), (7, 3), (80, 24), (200, 60)] {
                let fit = fit_in(w, h, cols, rows);
                assert!(
                    fit.cols <= cols && fit.rows <= rows,
                    "{w}×{h} into {cols}×{rows} produced {fit:?}"
                );
                assert!(
                    fit.cols >= 1 && fit.rows >= 1,
                    "{w}×{h} into {cols}×{rows} collapsed to nothing"
                );
            }
        }
    }

    #[test]
    fn degenerate_inputs_fit_into_nothing() {
        assert!(fit_in(0, 10, 10, 10).is_empty());
        assert!(fit_in(10, 0, 10, 10).is_empty());
        assert!(fit_in(10, 10, 0, 10).is_empty());
        assert!(fit_in(10, 10, 10, 0).is_empty());
    }

    #[test]
    fn alpha_blending_meets_both_endpoints() {
        assert_eq!(blend(255, 0, 255), 255, "opaque keeps the foreground");
        assert_eq!(
            blend(255, 22, 0),
            22,
            "fully transparent shows the backdrop"
        );
        assert_eq!(
            blend(255, 0, 128),
            128,
            "half alpha over black should round to the midpoint, not 127"
        );
    }

    /// Detection must never panic and must never claim a protocol the caller
    /// cannot fall back from.
    #[test]
    fn protocol_detection_is_total() {
        let protocol = detect_protocol();
        assert!(
            matches!(
                protocol,
                Protocol::Kitty | Protocol::Iterm2 | Protocol::HalfBlock
            ),
            "unexpected protocol {protocol:?}"
        );
    }
}
