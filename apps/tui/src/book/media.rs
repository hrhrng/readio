//! Pulling images out of an EPUB and onto disk.
//!
//! The renderer in [`crate::ui::image`] opens files by path, so the illustrations
//! inside the archive have to be unpacked once into a cache directory. Two
//! things make that more than a loop over zip entries: an EPUB references its
//! images with relative, percent-escaped hrefs that have to be resolved against
//! the document that mentioned them, and a zip is untrusted input — an entry
//! called `../../.ssh/authorized_keys` must never be written.
//!
//! Archive paths are keyed exactly as [`crate::book::epub`] keys them —
//! lowercase, forward slashes, no `.` segments — so a href resolved from an
//! XHTML document looks up the same entry the loader saw.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

/// Largest single image worth unpacking. Anything above this is decoration a
/// terminal could never show usefully, and unpacking it just burns disk.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Extensions we are willing to unpack. SVG is deliberately absent: it is
/// vector XML, and rasterising it needs a font and path renderer we do not have.
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Extract the images an EPUB references into `dest`, returning
/// archive-path → file-path.
///
/// Every entry is checked twice — extension *and* magic bytes — because an
/// EPUB's manifest lies often enough (`.jpg` files holding PNG data, `.png`
/// files holding SVG) that trusting either alone means feeding the decoder
/// rubbish. Failures are skips, not errors: one broken illustration must not
/// stop a book from opening. Only unreadable archives error.
pub fn extract_epub_images(epub: &Path, dest: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let file = std::fs::File::open(epub)
        .with_context(|| crate::i18n::tf("media.epub_unopenable", &[&epub.display()]))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| crate::i18n::tf("media.not_zip", &[&epub.display()]))?;
    std::fs::create_dir_all(dest)
        .with_context(|| crate::i18n::tf("media.cache_failed", &[&dest.display()]))?;

    let mut out: BTreeMap<String, PathBuf> = BTreeMap::new();
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        if entry.is_dir() || entry.size() == 0 || entry.size() > MAX_IMAGE_BYTES {
            continue;
        }
        // `name` is attacker-controlled; `safe_key` is the only path allowed
        // past here.
        let Some(key) = safe_key(entry.name()) else {
            continue;
        };
        if !has_image_extension(&key) {
            continue;
        }

        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut bytes).is_err() || sniff(&bytes).is_none() {
            continue;
        }

        let Some(path) = safe_dest(dest, &key) else {
            continue;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Idempotent: a cache hit of the same size is treated as the same
        // image, so reopening a book does not rewrite every illustration.
        let cached = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !cached && std::fs::write(&path, &bytes).is_err() {
            continue;
        }
        out.insert(key, path);
    }
    Ok(out)
}

/// Resolve an `<img src>` against the document that referenced it, returning
/// the archive key to look up.
///
/// Handles the three shapes real EPUBs use: relative with `..`
/// (`../images/a.png` from `OEBPS/text/c1.xhtml`), relative to the document's
/// own directory (`./a.png`), and rooted at the archive (`/images/a.png`).
/// Percent escapes are decoded, since zip entry names hold the literal bytes.
pub fn resolve_href(doc_href: &str, src: &str) -> String {
    let src = percent_decode(strip_fragment(src));
    if src.starts_with('/') {
        return normalize(&src);
    }
    let base = parent_dir(&normalize(doc_href));
    if base.is_empty() {
        normalize(&src)
    } else {
        normalize(&format!("{base}/{src}"))
    }
}

/// Lowercase, forward-slashed, `.`- and `..`-free key for archive lookups.
///
/// Matches `epub::normalize` so hrefs and entry names meet in one namespace.
fn normalize(name: &str) -> String {
    let slashed = name.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in slashed.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/").to_ascii_lowercase()
}

/// Normalised key for an archive entry, or `None` if the entry is trying to
/// climb out of the archive.
///
/// [`normalize`] silently swallows `..` by popping, which is right for href
/// lookups but wrong for extraction: `../../evil.png` would quietly become
/// `evil.png` and get written. So the raw name is walked first, and any segment
/// that would step above the root — or an absolute path, or a Windows drive —
/// rejects the entry outright.
fn safe_key(name: &str) -> Option<String> {
    let slashed = name.replace('\\', "/");
    if slashed.starts_with('/') || slashed.contains(':') {
        return None;
    }
    let mut depth = 0i32;
    for part in slashed.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => depth += 1,
        }
    }
    let key = normalize(&slashed);
    if key.is_empty() { None } else { Some(key) }
}

/// Where `key` may be written, or `None` if the join would land outside `dest`.
///
/// The second half of the zip-slip defence, and the one that does not depend on
/// [`safe_key`] having been called: the joined path must be made only of plain
/// components and must still start with `dest`. Checked lexically because
/// `canonicalize` cannot be used on a file that does not exist yet.
fn safe_dest(dest: &Path, key: &str) -> Option<PathBuf> {
    let relative = Path::new(key);
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let joined = dest.join(relative);
    joined.starts_with(dest).then_some(joined)
}

/// Whether an archive entry's name suggests a bitmap. Cheap pre-check so a book
/// with no pictures never pays for an extraction pass.
pub fn looks_like_image_name(name: &str) -> bool {
    has_image_extension(&normalize(name))
}

fn has_image_extension(key: &str) -> bool {
    let ext = key.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    IMAGE_EXTENSIONS.contains(&ext)
}

/// Format implied by the leading bytes of a file, if it is a raster image we
/// can decode. `None` covers SVG, CSS, XHTML, fonts and truncated junk alike.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.starts_with(&PNG) {
        return Some("png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    // RIFF containers hold anything; only the WEBP form is an image.
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }
    None
}

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn strip_fragment(href: &str) -> &str {
    let href = href.split('#').next().unwrap_or(href);
    href.split('?').next().unwrap_or(href)
}

/// Decode `%XX` escapes. Invalid escapes are left alone rather than dropped: a
/// filename containing a literal `%` is likelier than a mangled one.
fn percent_decode(src: &str) -> String {
    let raw = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == b'%'
            && i + 2 < raw.len()
            && let (Some(hi), Some(lo)) = (hex(raw[i + 1]), hex(raw[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
            continue;
        }
        out.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrefs_resolve_against_the_referencing_document() {
        assert_eq!(
            resolve_href("OEBPS/text/c1.xhtml", "../images/a.png"),
            "oebps/images/a.png"
        );
        assert_eq!(
            resolve_href("OEBPS/text/c1.xhtml", "./a.png"),
            "oebps/text/a.png"
        );
        assert_eq!(
            resolve_href("OEBPS/c1.xhtml", "images/a%20b.png"),
            "oebps/images/a b.png",
            "percent escapes must be decoded to match the zip entry name"
        );
        assert_eq!(
            resolve_href("OEBPS/text/c1.xhtml", "/images/a.png"),
            "images/a.png",
            "a leading slash means the archive root, not the document's dir"
        );
        assert_eq!(
            resolve_href("c1.xhtml", "a.png"),
            "a.png",
            "a document at the root has no directory to prepend"
        );
    }

    /// Anchors and query strings appear in hrefs copied from web sources; they
    /// are not part of the entry name.
    #[test]
    fn hrefs_drop_fragments_and_queries() {
        assert_eq!(resolve_href("t/c.xhtml", "a.png#frag"), "t/a.png");
        assert_eq!(resolve_href("t/c.xhtml", "a.png?v=2"), "t/a.png");
    }

    #[test]
    fn escaping_entry_names_are_refused_not_flattened() {
        for evil in [
            "../../evil.png",
            "a/../../evil.png",
            "/etc/passwd.png",
            "C:/windows/evil.png",
        ] {
            assert!(
                safe_key(evil).is_none(),
                "{evil} must be rejected outright, not normalised into place"
            );
        }
        assert_eq!(
            safe_key("OEBPS/img/../img/a.PNG").as_deref(),
            Some("oebps/img/a.png"),
            "a `..` that stays inside the archive is still a valid entry"
        );
    }

    #[test]
    fn destinations_stay_under_the_cache_dir() {
        let dest = Path::new("/tmp/cache");
        assert_eq!(
            safe_dest(dest, "img/a.png"),
            Some(PathBuf::from("/tmp/cache/img/a.png"))
        );
        assert!(
            safe_dest(dest, "../a.png").is_none(),
            "the lexical check must hold even if safe_key were bypassed"
        );
        assert!(safe_dest(dest, "/a.png").is_none());
    }

    #[test]
    fn only_raster_magic_bytes_count_as_images() {
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0]),
            Some("png")
        );
        assert_eq!(sniff(b"GIF89a...."), Some("gif"));
        assert_eq!(sniff(b"RIFF____WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"RIFF____WAVEfmt "), None, "a WAV is not an image");
        assert_eq!(sniff(br#"<svg xmlns="...">"#), None, "SVG is out of scope");
        assert_eq!(sniff(b""), None);
        assert_eq!(
            sniff(&[0x89, b'P']),
            None,
            "a truncated header is not a PNG"
        );
    }

    #[test]
    fn extensions_are_matched_case_insensitively_via_the_key() {
        assert!(has_image_extension("a/b.png"));
        assert!(has_image_extension(&normalize("A/B.JPEG")));
        assert!(!has_image_extension("a/b.svg"));
        assert!(!has_image_extension("a/b"));
    }
}
