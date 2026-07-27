//! EPUB loading: container → OPF → spine → XHTML documents.
//!
//! The whole archive is slurped into memory up front. EPUBs are a few
//! megabytes at worst, and holding the bytes lets us resolve hrefs
//! case-insensitively without fighting the zip reader's borrow rules.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use quick_xml::Reader;
use quick_xml::events::Event;

use super::{Book, Chapter, Para, Source, html, media};

pub fn load(path: &Path) -> Result<Book> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("{} is not a valid EPUB (zip) file", path.display()))?;

    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        entries.insert(normalize(&name), bytes);
    }

    let container = read_str(&entries, "META-INF/container.xml")
        .ok_or_else(|| anyhow!("EPUB is missing META-INF/container.xml"))?;
    let opf_path = find_attr(&container, "rootfile", "full-path")
        .ok_or_else(|| anyhow!("EPUB container.xml has no rootfile"))?;
    let opf = read_str(&entries, &opf_path)
        .ok_or_else(|| anyhow!("EPUB package file {opf_path} not found"))?;
    let base = parent_dir(&opf_path);

    let package = parse_opf(&opf);
    let mut chapters = Vec::new();

    // Titles from the NCX / nav document, keyed by document href.
    let toc_titles = package
        .toc_href
        .as_ref()
        .and_then(|href| read_str(&entries, &join(&base, href)))
        .map(|doc| parse_toc(&doc))
        .unwrap_or_default();

    // Illustrations are extracted once, up front: rendering then only opens
    // plain files, and a book with no images pays nothing for the attempt.
    let images = extracted_images(path, &entries);

    for idref in &package.spine {
        let Some(item) = package.manifest.get(idref) else {
            continue;
        };
        if !item.media_type.contains("html") {
            continue;
        }
        let full = join(&base, &item.href);
        let Some(doc) = read_str(&entries, &full) else {
            continue;
        };
        let mut paras = html::to_paras(&doc);
        resolve_images(&mut paras, &full, &images);
        if paras.is_empty() {
            continue;
        }
        // A spine document need not announce its own name. Prefer what the ToC
        // calls it, then its first heading, and only then fall back — to a
        // section number, never to the filename, because `index_split_004` is
        // a fact about the publisher's toolchain and not about the book.
        let title = toc_titles
            .get(&normalize(&item.href))
            .or_else(|| toc_titles.get(&normalize(&full)))
            .cloned()
            .or_else(|| html::first_heading(&paras).filter(|h| h.chars().count() <= 60))
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| crate::i18n::tf("book.section", &[&(chapters.len() + 1)]));

        chapters.push(Chapter {
            title,
            href: full,
            paras,
        });
    }

    if chapters.is_empty() {
        return Err(anyhow!("EPUB contains no readable chapters"));
    }

    let title = package
        .title
        .unwrap_or_else(|| file_stem(&path.to_string_lossy()));
    Ok(Book {
        id: String::new(),
        title,
        author: package.author,
        path: Some(path.to_path_buf()),
        source: Source::Epub,
        chapters,
    })
}

/// Extract the archive's images into the host directory, keyed by archive path.
///
/// A book without pictures, or an unwritable cache, simply yields an empty map:
/// missing illustrations must never stop a book from opening.
fn extracted_images(path: &Path, entries: &HashMap<String, Vec<u8>>) -> BTreeMap<String, PathBuf> {
    let has_images = entries.keys().any(|key| media::looks_like_image_name(key));
    if !has_images {
        return BTreeMap::new();
    }
    let dest = crate::paths::images_dir().join(crate::book::synthetic_id(&path.to_string_lossy()));
    media::extract_epub_images(path, &dest).unwrap_or_default()
}

/// Point every image paragraph at the file that was extracted for it, and drop
/// the ones with nothing behind them — a caption for a picture that cannot be
/// shown is worse than no picture.
fn resolve_images(paras: &mut Vec<Para>, doc_href: &str, images: &BTreeMap<String, PathBuf>) {
    paras.retain_mut(|para| {
        let Para::Image { src, alt } = para else {
            return true;
        };
        let raw = src.to_string_lossy().into_owned();
        let key = media::resolve_href(doc_href, &raw);
        match images.get(&key) {
            Some(file) => {
                *src = file.clone();
                true
            }
            // An unresolvable image with a caption still says something.
            None => {
                if alt.trim().is_empty() {
                    false
                } else {
                    *para = Para::Text(alt.clone());
                    true
                }
            }
        }
    });
}

// ── OPF ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Package {
    title: Option<String>,
    author: Option<String>,
    manifest: HashMap<String, Item>,
    spine: Vec<String>,
    toc_href: Option<String>,
}

#[derive(Debug)]
struct Item {
    href: String,
    media_type: String,
}

fn parse_opf(opf: &str) -> Package {
    let mut pkg = Package::default();
    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    // Which metadata element we are inside, if any.
    let mut field: Option<&'static str> = None;
    let mut toc_id: Option<String> = None;
    let mut nav_href: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag = local(e.name().as_ref());
                let attrs = attrs(&e);
                match tag.as_str() {
                    "title" => field = Some("title"),
                    "creator" => field = Some("creator"),
                    "item" => {
                        let id = attrs.get("id").cloned().unwrap_or_default();
                        let href = attrs.get("href").cloned().unwrap_or_default();
                        let media_type = attrs.get("media-type").cloned().unwrap_or_default();
                        let properties = attrs.get("properties").cloned().unwrap_or_default();
                        if properties.split_whitespace().any(|p| p == "nav") {
                            nav_href = Some(href.clone());
                        }
                        if !id.is_empty() {
                            pkg.manifest.insert(id, Item { href, media_type });
                        }
                    }
                    "spine" => {
                        if let Some(toc) = attrs.get("toc") {
                            toc_id = Some(toc.clone());
                        }
                    }
                    "itemref" => {
                        if let Some(idref) = attrs.get("idref") {
                            pkg.spine.push(idref.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(which) = field.take() {
                    let value = e.decode().map(|c| c.trim().to_string()).unwrap_or_default();
                    if value.is_empty() {
                        continue;
                    }
                    match which {
                        "title" if pkg.title.is_none() => pkg.title = Some(value),
                        "creator" if pkg.author.is_none() => pkg.author = Some(value),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => field = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    pkg.toc_href = toc_id
        .and_then(|id| pkg.manifest.get(&id).map(|i| i.href.clone()))
        .or(nav_href);
    pkg
}

/// Map document href → chapter title, from either an NCX or an EPUB 3 nav doc.
fn parse_toc(doc: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut reader = Reader::from_str(doc);
    reader.config_mut().check_end_names = false;
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    // NCX: <navLabel><text>T</text></navLabel><content src="H"/>
    // Nav: <a href="H">T</a>
    let mut pending_label: Option<String> = None;
    let mut in_text = false;
    let mut anchor: Option<(String, String)> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag = local(e.name().as_ref());
                let attrs = attrs(&e);
                match tag.as_str() {
                    "text" => in_text = true,
                    "content" => {
                        if let (Some(src), Some(label)) = (attrs.get("src"), pending_label.take()) {
                            out.insert(normalize(strip_fragment(src)), label);
                        }
                    }
                    "a" => {
                        if let Some(href) = attrs.get("href") {
                            anchor = Some((normalize(strip_fragment(href)), String::new()));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let value = e.decode().map(|c| c.trim().to_string()).unwrap_or_default();
                if value.is_empty() {
                    continue;
                }
                if in_text {
                    pending_label = Some(value.clone());
                }
                if let Some((_, label)) = anchor.as_mut() {
                    label.push_str(&value);
                }
            }
            Ok(Event::End(e)) => {
                let tag = local(e.name().as_ref());
                if tag == "text" {
                    in_text = false;
                }
                if tag == "a"
                    && let Some((href, label)) = anchor.take()
                    && !label.trim().is_empty()
                {
                    out.insert(href, label.trim().to_string());
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

// ── path + attribute helpers ─────────────────────────────────────────────────

fn attrs(e: &quick_xml::events::BytesStart<'_>) -> HashMap<String, String> {
    e.attributes()
        .flatten()
        .filter_map(|a| {
            let key = local(a.key.as_ref());
            let value = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()?
                .into_owned();
            Some((key, value))
        })
        .collect()
}

fn local(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

/// Single-pass scan for the first `<tag attr="…">` in a document.
fn find_attr(doc: &str, tag: &str, attr: &str) -> Option<String> {
    let mut reader = Reader::from_str(doc);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if local(e.name().as_ref()) == tag
                    && let Some(v) = attrs(&e).get(attr)
                {
                    return Some(v.clone());
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

fn read_str(entries: &HashMap<String, Vec<u8>>, name: &str) -> Option<String> {
    let bytes = entries.get(&normalize(name))?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Lowercase, forward-slashed, `./`-free key for archive lookups.
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

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn join(base: &str, href: &str) -> String {
    let href = strip_fragment(href);
    if base.is_empty() || href.starts_with('/') {
        return href.trim_start_matches('/').to_string();
    }
    format!("{base}/{href}")
}

fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(name)
        .to_string()
}
