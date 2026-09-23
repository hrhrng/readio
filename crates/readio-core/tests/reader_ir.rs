use readio_core::{
    documents::{HtmlParseOptions, parse_html},
    reader::{
        ReaderChapterSource, ReaderNavigationSource, compose_reader_ir, parse_html_reader,
        parse_markdown_reader, parse_plain_text_reader,
    },
};

#[test]
fn reader_ir_is_the_single_document_and_navigation_source_for_gui_projection() {
    let first = parse_html(
        "<p>Same.</p><p>🙂 hello, 世界!</p>",
        &HtmlParseOptions::default(),
    )
    .expect("first chapter");
    let second = parse_html("<p>Same.</p>", &HtmlParseOptions::default()).expect("second chapter");
    let second_target = second.blocks[0].id.clone();

    let reader = compose_reader_ir(
        b"stable-publication-bytes",
        "fixture-reader-ir-1",
        vec![
            ReaderChapterSource {
                id: "chapter-1".to_string(),
                title: "One".to_string(),
                document: first,
            },
            ReaderChapterSource {
                id: "chapter-2".to_string(),
                title: "Two".to_string(),
                document: second,
            },
        ],
        vec![ReaderNavigationSource {
            id: "toc-two".to_string(),
            title: "Two".to_string(),
            depth: 0,
            chapter_id: "chapter-2".to_string(),
            block_id: Some(second_target),
        }],
    );

    assert_eq!(reader.ir_version, 1);
    assert_eq!(reader.document.blocks.len(), 3);
    assert_ne!(reader.document.blocks[0].id, reader.document.blocks[2].id);
    assert_eq!(
        reader.text_index.document_revision,
        reader.document.revision.revision_digest
    );
    assert_eq!(
        reader
            .text_index
            .sentences
            .iter()
            .map(|sentence| (sentence.index, sentence.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "Same."), (1, "🙂 hello, 世界!"), (2, "Same.")]
    );
    assert_eq!(reader.chapters[0].block_range, [0, 2]);
    assert_eq!(reader.chapters[0].sentence_range, [0, 2]);
    assert_eq!(reader.chapters[1].block_range, [2, 3]);
    assert_eq!(reader.chapters[1].sentence_range, [2, 3]);
    assert_eq!(
        reader.navigation[0].block_id.as_deref(),
        Some(reader.document.blocks[2].id.as_str())
    );
}

#[test]
fn html_reader_ir_owns_document_sentences_and_heading_navigation() {
    let reader = parse_html_reader(
        "<h2 id='start'>Start 🙂</h2><p>One. Two.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("HTML reader IR");

    assert_eq!(reader.ir_version, 1);
    assert_eq!(reader.document.blocks.len(), 2);
    assert_eq!(
        reader
            .text_index
            .sentences
            .iter()
            .map(|sentence| sentence.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Start 🙂", "One.", "Two."]
    );
    assert_eq!(reader.chapters[0].block_range, [0, 2]);
    assert_eq!(reader.chapters[0].sentence_range, [0, 3]);
    assert_eq!(reader.navigation.len(), 1);
    assert_eq!(reader.navigation[0].title, "Start 🙂");
    assert_eq!(reader.navigation[0].depth, 1);
    assert_eq!(reader.navigation[0].sentence_index, Some(0));
    assert_eq!(
        reader.navigation[0].block_id.as_deref(),
        Some(reader.document.blocks[0].id.as_str())
    );
}

#[test]
fn plain_text_reader_ir_owns_paragraph_and_sentence_boundaries() {
    let reader = parse_plain_text_reader(
        " First one. Second one.\ncontinued\n\n第二段。🙂 done! ",
        &HtmlParseOptions::default(),
    )
    .expect("plain-text reader IR");

    assert_eq!(reader.document.blocks.len(), 2);
    assert_eq!(
        reader.document.blocks[0].text,
        "First one. Second one. continued"
    );
    assert_eq!(reader.document.blocks[1].text, "第二段。🙂 done!");
    assert_eq!(
        reader
            .text_index
            .sentences
            .iter()
            .map(|sentence| sentence.text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "First one.",
            "Second one.",
            "continued",
            "第二段。",
            "🙂 done!"
        ]
    );
    assert_eq!(reader.chapters[0].block_range, [0, 2]);
    assert_eq!(reader.chapters[0].sentence_range, [0, 5]);
}

#[test]
fn markdown_reader_ir_preserves_blocks_navigation_and_inline_emphasis() {
    let reader = parse_markdown_reader(
        "# Quiet tools\n\nRead **one thing** well.\n\n> Attention has a shape.\n\n```rs\nlet calm = true;\n```",
        &HtmlParseOptions::default(),
    )
    .expect("Markdown reader IR");

    assert_eq!(
        reader
            .document
            .blocks
            .iter()
            .map(|block| (&block.kind, block.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                &readio_core::documents::BlockKind::Heading { level: 1 },
                "Quiet tools"
            ),
            (
                &readio_core::documents::BlockKind::Paragraph,
                "Read one thing well."
            ),
            (
                &readio_core::documents::BlockKind::Quote,
                "Attention has a shape."
            ),
            (&readio_core::documents::BlockKind::Code, "let calm = true;"),
        ]
    );
    assert_eq!(reader.document.blocks[1].inline_spans.len(), 1);
    assert_eq!(reader.document.blocks[1].inline_spans[0].range, [5, 14]);
    assert!(reader.document.blocks[1].inline_spans[0].bold);
    assert_eq!(reader.navigation[0].title, "Quiet tools");
    assert_eq!(reader.navigation[0].sentence_index, Some(0));
    assert_eq!(reader.text_index.sentences.len(), 3);
}

#[test]
fn markdown_reader_ir_preserves_safe_relative_images_and_rejects_active_urls() {
    let reader = parse_markdown_reader(
        "![Local figure](images/figure.png)\n\n![Unsafe](javascript:alert(1))",
        &HtmlParseOptions::default(),
    )
    .expect("Markdown reader IR");

    assert_eq!(reader.document.assets.len(), 1);
    assert_eq!(reader.document.assets[0].source, "images/figure.png");
    assert_eq!(reader.document.assets[0].alt, "Local figure");
    assert!(matches!(
        reader.document.blocks[0].kind,
        readio_core::documents::BlockKind::Image { .. }
    ));
    assert!(
        reader
            .document
            .blocks
            .iter()
            .all(|block| block.text != "Unsafe")
    );
}

#[test]
fn reader_text_index_excludes_non_prose_code_and_image_alt_text() {
    let reader = parse_html_reader(
        "<p>Read this.</p><pre>let hidden = true;</pre><img src='https://example.com/a.png' alt='Diagram'>",
        &HtmlParseOptions::default(),
    )
    .expect("reader IR");

    assert_eq!(reader.document.blocks.len(), 3);
    assert_eq!(reader.text_index.sentences.len(), 1);
    assert_eq!(reader.text_index.sentences[0].text, "Read this.");
}
