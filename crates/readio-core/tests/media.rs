#![cfg(feature = "native-media")]

use std::io::{Cursor, Write};

use readio_core::media::{MediaError, extract_epub_asset, parse_epub, parse_pdf};
use serde_json::Value;

const COVER_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ncover";
const FIGURE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nfigure";

#[test]
fn pdf_parser_returns_metadata_shared_document_and_page_provenance() {
    let publication = parse_pdf(&minimal_text_pdf(&[
        "Chapter One",
        "The first line of readable prose keeps going long enough",
        "to become one reconstructed paragraph in the reader.",
    ]))
    .expect("text PDF publication");
    let value = serde_json::to_value(publication).expect("PDF publication JSON");

    assert_eq!(value["title"], "Readable PDF");
    assert_eq!(value["author"], "Ada Reader");
    assert_eq!(value["page_count"], 1);
    assert_eq!(value["chapters"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["chapters"][0]["title"], Value::Null);
    assert_eq!(value["chapters"][0]["page_start"], 1);
    assert_eq!(value["chapters"][0]["page_end"], 1);
    assert_eq!(
        value["chapters"][0]["document"]["revision"]["parser_version"],
        "pdf-ir-1"
    );
    assert_eq!(
        value["chapters"][0]["document"]["revision"]["char_count"],
        121
    );
    assert_eq!(
        value["chapters"][0]["document"]["blocks"][0]["kind"],
        serde_json::json!({ "type": "heading", "level": 2 })
    );
    assert_eq!(
        value["chapters"][0]["block_provenance"]
            .as_array()
            .map(Vec::len),
        value["chapters"][0]["document"]["blocks"]
            .as_array()
            .map(Vec::len)
    );
    assert!(
        value["chapters"][0]["block_provenance"]
            .as_array()
            .is_some_and(|provenance| provenance.iter().all(|source| {
                source["page_start"] == 1
                    && source["page_end"] == 1
                    && source["block_id"].as_str().is_some_and(|id| !id.is_empty())
            })),
        "every shared block must retain the page it came from: {value}"
    );
    assert_eq!(
        value["canonical_text"],
        "Chapter One\nThe first line of readable prose keeps going long enough to become one reconstructed paragraph in the reader."
    );
    assert_eq!(value["reader"]["ir_version"], 1);
    assert_eq!(
        value["reader"]["document"]["revision"]["parser_version"],
        "pdf-reader-ir-1"
    );
    assert_eq!(
        value["reader"]["text_index"]["document_revision"],
        value["reader"]["document"]["revision"]["revision_digest"]
    );
    assert_eq!(
        value["reader"]["chapters"][0]["block_range"],
        serde_json::json!([0, 2])
    );
    assert_eq!(
        value["reader"]["chapters"][0]["sentence_range"],
        serde_json::json!([0, 2])
    );
}

#[test]
fn pdf_parser_removes_repeated_page_furniture_without_eating_body_text() {
    let page_names = ["Amber", "Birch", "Cedar", "Dune"];
    let pages = page_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let page = index + 1;
            vec![
                "Readio Manual".to_string(),
                format!("{name} starts with unique readable prose."),
                "The same sentence belongs in every page body and must survive.".to_string(),
                format!("{name} ends with another unique sentence."),
                format!("Page {page} of 4"),
            ]
        })
        .collect::<Vec<_>>();
    let borrowed = pages
        .iter()
        .map(|page| page.iter().map(String::as_str).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let publication = parse_pdf(&text_pdf(&borrowed)).expect("four-page text PDF");

    assert!(!publication.canonical_text.contains("Readio Manual"));
    assert!(!publication.canonical_text.contains("Page 1 of 4"));
    assert!(!publication.canonical_text.contains("Page 4 of 4"));
    assert_eq!(
        publication
            .canonical_text
            .matches("The same sentence belongs in every page body and must survive.")
            .count(),
        4,
        "a repeated body sentence is content, not page furniture"
    );
    for name in page_names {
        assert!(
            publication
                .canonical_text
                .contains(&format!("{name} starts")),
            "{name} page body must survive"
        );
    }
}

#[test]
fn pdf_parser_splits_detected_headings_only_when_each_has_prose() {
    let publication = parse_pdf(&text_pdf(&[
        vec![
            "Chapter One",
            "The first chapter has enough readable prose for extraction.",
        ],
        vec![
            "Chapter Two",
            "The second chapter also has enough readable prose to keep.",
        ],
    ]))
    .expect("heading-based PDF chapters");

    assert_eq!(
        publication
            .chapters
            .iter()
            .map(|chapter| chapter.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Chapter One"), Some("Chapter Two")]
    );
    assert_eq!(
        publication
            .chapters
            .iter()
            .map(|chapter| (chapter.page_start, chapter.page_end))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 2)]
    );
    assert!(publication.chapters.iter().all(|chapter| {
        chapter
            .document
            .blocks
            .iter()
            .any(|block| matches!(block.kind, readio_core::documents::BlockKind::Paragraph))
    }));

    let contents = parse_pdf(&text_pdf(&[
        vec!["Contents", "Chapter One", "Chapter Two", "Chapter Three"],
        vec!["The actual body starts here and ends as readable prose."],
    ]))
    .expect("PDF table of contents");
    assert_eq!(
        contents.chapters.len(),
        1,
        "a run of headings without prose between them is a table of contents"
    );
}

#[test]
fn pdf_parser_falls_back_to_stable_page_aligned_chapter_ranges() {
    let storage = (b'a'..=b'f')
        .map(|character| format!("{}.", (character as char).to_string().repeat(1200)))
        .collect::<Vec<_>>();
    let pages = storage
        .iter()
        .map(|line| vec![line.as_str()])
        .collect::<Vec<_>>();
    let publication = parse_pdf(&text_pdf(&pages)).expect("long heading-free PDF");

    assert_eq!(
        publication
            .chapters
            .iter()
            .map(|chapter| (
                chapter.title.as_deref(),
                chapter.page_start,
                chapter.page_end
            ))
            .collect::<Vec<_>>(),
        vec![(None, 1, 3), (None, 4, 6)]
    );
    assert_eq!(
        publication
            .chapters
            .iter()
            .map(|chapter| chapter.document.blocks.len())
            .sum::<usize>(),
        6,
        "fallback grouping must not drop or duplicate page blocks"
    );
}

#[test]
fn pdf_parser_prefers_outline_titles_over_detected_headings() {
    let pages = [
        vec![
            "Chapter Detected One",
            "The first outlined chapter contains readable prose.",
        ],
        vec![
            "Chapter Detected Two",
            "The second outlined chapter contains readable prose.",
        ],
    ];
    let publication = parse_pdf(&text_pdf_with_outline(
        &pages,
        &[("Bookmark Alpha", 1), ("Bookmark Beta", 2)],
    ))
    .expect("outlined PDF");

    assert_eq!(
        publication
            .chapters
            .iter()
            .map(|chapter| (
                chapter.title.as_deref(),
                chapter.page_start,
                chapter.page_end
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("Bookmark Alpha"), 1, 1),
            (Some("Bookmark Beta"), 2, 2),
        ]
    );
}

#[test]
fn epub_two_ncx_preserves_document_order_depth_and_fragment_targets() {
    let publication = parse_epub(&epub_two_fixture()).expect("EPUB 2 publication");

    assert_eq!(
        publication
            .toc
            .iter()
            .map(|entry| (entry.title.as_str(), entry.depth, entry.chapter_id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("The chapter", 0, "chapter"),
            ("Deep section", 1, "chapter"),
        ]
    );
    assert_eq!(publication.toc[0].block_id, None);
    assert_eq!(
        publication.toc[1].block_id.as_deref(),
        Some(publication.chapters[0].document.blocks[1].id.as_str())
    );
    assert_eq!(publication.reader.ir_version, 1);
    assert_eq!(publication.reader.chapters[0].block_range, [0, 3]);
    assert_eq!(publication.reader.navigation.len(), 2);
    assert_eq!(publication.reader.navigation[0].sentence_index, Some(0));
    assert_eq!(publication.reader.navigation[1].sentence_index, Some(1));
    assert_eq!(
        publication.reader.navigation[1].block_id.as_deref(),
        Some(publication.reader.document.blocks[1].id.as_str())
    );
}

#[test]
fn epub_images_are_stable_archive_assets_and_cover_metadata_points_to_one() {
    let publication = parse_epub(&epub_two_fixture()).expect("EPUB 2 publication");
    let value = serde_json::to_value(publication).expect("publication JSON");
    let assets = value["assets"].as_array().expect("EPUB asset array");

    assert_eq!(
        assets
            .iter()
            .map(|asset| {
                (
                    asset["href"].as_str().expect("asset href"),
                    asset["media_type"].as_str().expect("asset media type"),
                    asset["byte_length"].as_u64().expect("asset byte length"),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "ops/images/cover.png",
                "image/png",
                COVER_BYTES.len() as u64
            ),
            (
                "ops/images/figure.png",
                "image/png",
                FIGURE_BYTES.len() as u64,
            ),
        ]
    );

    let cover_id = value["cover_asset_id"].as_str().expect("cover asset ID");
    assert_eq!(cover_id, assets[0]["id"].as_str().expect("cover ID"));

    let figure_id = assets[1]["id"].as_str().expect("figure ID");
    let document_assets = value["chapters"][0]["document"]["assets"]
        .as_array()
        .expect("chapter document assets");
    assert_eq!(document_assets.len(), 1);
    assert_eq!(
        document_assets[0]["id"],
        Value::String(figure_id.to_string())
    );
    assert_eq!(
        document_assets[0]["source"],
        Value::String("ops/images/figure.png".to_string())
    );
    assert_eq!(
        document_assets[0]["alt"],
        Value::String("A diagram".to_string())
    );
    assert_eq!(
        value["chapters"][0]["document"]["blocks"][2]["kind"]["asset_id"],
        Value::String(figure_id.to_string())
    );
}

#[test]
fn epub_asset_bytes_are_loaded_on_demand_from_the_original_archive() {
    let epub = epub_two_fixture();
    let publication = parse_epub(&epub).expect("EPUB 2 publication");
    let figure = publication
        .assets
        .iter()
        .find(|asset| asset.href == "ops/images/figure.png")
        .expect("figure metadata");

    assert_eq!(
        extract_epub_asset(&epub, &figure.id).expect("figure bytes"),
        FIGURE_BYTES
    );
}

#[test]
fn epub_asset_extraction_rejects_ids_not_declared_by_the_package() {
    let error = extract_epub_asset(&epub_two_fixture(), "epub-asset-not-declared")
        .expect_err("unknown assets must not address archive paths");

    assert_eq!(
        error,
        MediaError::EpubAssetNotFound {
            asset_id: "epub-asset-not-declared".to_string(),
        }
    );
}

#[test]
fn epub_asset_extraction_rejects_an_image_too_large_for_interactive_reading() {
    let epub = epub_two_fixture_with_figure(&vec![0_u8; 40 * 1024 * 1024]);
    let publication = parse_epub(&epub).expect("EPUB with oversized image metadata");
    let figure_id = publication
        .assets
        .iter()
        .find(|asset| asset.href == "ops/images/figure.png")
        .expect("oversized figure metadata")
        .id
        .clone();

    assert!(
        extract_epub_asset(&epub, &figure_id).is_err(),
        "archive image extraction must enforce a bounded allocation"
    );
}

fn epub_two_fixture() -> Vec<u8> {
    epub_two_fixture_with_figure(FIGURE_BYTES)
}

fn epub_two_fixture_with_figure(figure_bytes: &[u8]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        let entries = [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
                  <rootfiles><rootfile full-path="OPS/content.opf" /></rootfiles>
                </container>"#,
            ),
            (
                "OPS/content.opf",
                r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
                  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                    <dc:title>EPUB Two</dc:title>
                    <meta name="cover" content="cover-image" />
                  </metadata>
                  <manifest>
                    <item id="chapter" href="Text/chapter.xhtml" media-type="application/xhtml+xml" />
                    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml" />
                    <item id="cover-image" href="Images/cover.png" media-type="image/png" />
                    <item id="figure" href="Images/figure.png" media-type="image/png" />
                  </manifest>
                  <spine toc="ncx"><itemref idref="chapter" /></spine>
                </package>"#,
            ),
            (
                "OPS/Text/chapter.xhtml",
                "<html><body><h1>The chapter</h1><h2 id=\"deep\">Deep section</h2><img src=\"../Images/figure.png\" alt=\"A diagram\"></body></html>",
            ),
            (
                "OPS/toc.ncx",
                r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
                  <navMap>
                    <navPoint id="chapter-point">
                      <navLabel><text>The chapter</text></navLabel>
                      <content src="Text/chapter.xhtml" />
                      <navPoint id="deep-point">
                        <navLabel><text>Deep section</text></navLabel>
                        <content src="Text/chapter.xhtml#deep" />
                      </navPoint>
                    </navPoint>
                  </navMap>
                </ncx>"#,
            ),
        ];
        for (name, contents) in entries {
            archive.start_file(name, options).expect("start EPUB entry");
            archive
                .write_all(contents.as_bytes())
                .expect("write EPUB entry");
        }
        for (name, contents) in [
            ("OPS/Images/cover.png", COVER_BYTES),
            ("OPS/Images/figure.png", figure_bytes),
        ] {
            archive.start_file(name, options).expect("start EPUB asset");
            archive.write_all(contents).expect("write EPUB asset");
        }
        archive.finish().expect("finish EPUB");
    }
    bytes.into_inner()
}

fn minimal_text_pdf(lines: &[&str]) -> Vec<u8> {
    text_pdf(&[lines.to_vec()])
}

fn text_pdf(pages: &[Vec<&str>]) -> Vec<u8> {
    text_pdf_with_outline(pages, &[])
}

fn text_pdf_with_outline(pages: &[Vec<&str>], outline: &[(&str, usize)]) -> Vec<u8> {
    let page_count = pages.len();
    let page_ids = (0..page_count).map(|index| 5 + index).collect::<Vec<_>>();
    let stream_ids = (0..page_count)
        .map(|index| 5 + page_count + index)
        .collect::<Vec<_>>();
    let outline_root_id = 5 + page_count * 2;
    let kids = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let catalog = if outline.is_empty() {
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()
    } else {
        format!("<< /Type /Catalog /Pages 2 0 R /Outlines {outline_root_id} 0 R >>").into_bytes()
    };
    let mut objects = vec![
        catalog,
        format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>").into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        b"<< /Title (Readable PDF) /Author (Ada Reader) >>".to_vec(),
    ];
    for stream_id in &stream_ids {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {stream_id} 0 R >>"
            )
            .into_bytes(),
        );
    }
    for lines in pages {
        let mut text = String::from("BT\n/F1 12 Tf\n16 TL\n72 720 Td\n");
        for line in lines {
            let escaped = line
                .replace('\\', r"\\")
                .replace('(', r"\(")
                .replace(')', r"\)");
            text.push_str(&format!("({escaped}) Tj\nT*\n"));
        }
        text.push_str("ET\n");
        let stream = text.into_bytes();
        let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        content.extend_from_slice(&stream);
        content.extend_from_slice(b"\nendstream");
        objects.push(content);
    }
    if !outline.is_empty() {
        let first = outline_root_id + 1;
        let last = outline_root_id + outline.len();
        objects.push(
            format!(
                "<< /Type /Outlines /First {first} 0 R /Last {last} 0 R /Count {} >>",
                outline.len()
            )
            .into_bytes(),
        );
        for (index, (title, page)) in outline.iter().enumerate() {
            let object_id = outline_root_id + index + 1;
            let page_id = page_ids[*page - 1];
            let escaped = title
                .replace('\\', r"\\")
                .replace('(', r"\(")
                .replace(')', r"\)");
            let previous = (index > 0)
                .then(|| format!(" /Prev {} 0 R", object_id - 1))
                .unwrap_or_default();
            let next = (index + 1 < outline.len())
                .then(|| format!(" /Next {} 0 R", object_id + 1))
                .unwrap_or_default();
            objects.push(
                format!(
                    "<< /Title ({escaped}) /Parent {outline_root_id} 0 R /Dest [{page_id} 0 R /Fit]{previous}{next} >>"
                )
                .into_bytes(),
            );
        }
    }

    let mut output = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    output.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    output
}
