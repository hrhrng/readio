use readio_core::documents::{BlockKind, DocumentError, HtmlParseOptions, MediaKind, parse_html};
use serde_json::json;

#[test]
fn html_becomes_platform_neutral_semantic_blocks() {
    let html = r#"
        <!doctype html>
        <html><body>
          <h2>  A&nbsp;small   beginning </h2>
          <p>Hello <strong>world</strong>.  Next line.</p>
          <blockquote> Read this carefully. </blockquote>
          <pre>let x = 1;
x += 1;</pre>
        </body></html>
    "#;

    let document = parse_html(html, &HtmlParseOptions::default()).expect("valid HTML");

    assert_eq!(document.blocks.len(), 4);
    assert_eq!(document.blocks[0].kind, BlockKind::Heading { level: 2 });
    assert_eq!(document.blocks[0].text, "A small beginning");
    assert_eq!(document.blocks[1].kind, BlockKind::Paragraph);
    assert_eq!(document.blocks[1].text, "Hello world. Next line.");
    assert_eq!(document.blocks[2].kind, BlockKind::Quote);
    assert_eq!(document.blocks[2].text, "Read this carefully.");
    assert_eq!(document.blocks[3].kind, BlockKind::Code);
    assert_eq!(document.blocks[3].text, "let x = 1;\nx += 1;");
}

#[test]
fn html_preserves_inline_emphasis_as_normalized_unicode_scalar_ranges() {
    let html = r#"
        <p>前 <strong>粗🙂</strong> normal <em>斜中</em>
        <strong><em>both</em></strong>.</p>
    "#;

    let document = parse_html(html, &HtmlParseOptions::default()).expect("valid rich HTML");
    let block = serde_json::to_value(&document.blocks[0]).expect("block JSON");

    assert_eq!(block["text"], json!("前 粗🙂 normal 斜中 both."));
    assert_eq!(
        block["inline_spans"],
        json!([
            { "range": [2, 4], "bold": true, "italic": false },
            { "range": [12, 14], "bold": false, "italic": true },
            { "range": [15, 19], "bold": true, "italic": true }
        ])
    );
}

#[test]
fn html_source_anchors_resolve_to_the_first_semantic_block_in_their_scope() {
    let html = r#"
        <section id="introduction">
          <h2>Introduction</h2>
          <p>Opening.</p>
        </section>
        <h3 id="details">Details</h3>
    "#;

    let document = parse_html(html, &HtmlParseOptions::default()).expect("anchored HTML");
    let value = serde_json::to_value(&document).expect("document JSON");

    assert_eq!(
        value["anchors"],
        json!([
            {
                "source_id": "introduction",
                "block_id": document.blocks[0].id,
            },
            {
                "source_id": "details",
                "block_id": document.blocks[2].id,
            }
        ])
    );
}

#[test]
fn changing_only_a_source_anchor_changes_the_document_revision() {
    let first = parse_html(
        "<h2 id=\"first\">Same heading</h2>",
        &HtmlParseOptions::default(),
    )
    .expect("first document");
    let second = parse_html(
        "<h2 id=\"second\">Same heading</h2>",
        &HtmlParseOptions::default(),
    )
    .expect("second document");

    assert_ne!(
        first.revision.revision_digest,
        second.revision.revision_digest
    );
}

#[test]
fn html_drops_active_content_and_page_chrome() {
    let html = r#"
        <html><body>
          <nav><p>Navigation should disappear.</p></nav>
          <main>
            <p>Keep this<script>alert('x')</script><style>.x { color: red }</style> text.</p>
          </main>
          <footer><p>Footer should disappear.</p></footer>
        </body></html>
    "#;

    let document = parse_html(html, &HtmlParseOptions::default()).expect("valid HTML");

    assert_eq!(document.blocks.len(), 1);
    assert_eq!(document.blocks[0].kind, BlockKind::Paragraph);
    assert_eq!(document.blocks[0].text, "Keep this text.");
}

#[test]
fn html_resolves_safe_images_into_media_assets_and_drops_active_urls() {
    let html = r#"
        <html><body>
          <p>Before the illustration.</p>
          <img src="../images/cover%20one.jpg" alt="The cover">
          <img src="javascript:alert('x')" alt="Unsafe">
        </body></html>
    "#;
    let options = HtmlParseOptions {
        base_url: Some("https://reader.example/books/chapter/index.html".to_string()),
        ..HtmlParseOptions::default()
    };

    let document = parse_html(html, &options).expect("valid HTML");

    assert_eq!(document.assets.len(), 1);
    assert_eq!(document.assets[0].kind, MediaKind::Image);
    assert_eq!(
        document.assets[0].source,
        "https://reader.example/books/images/cover%20one.jpg"
    );
    assert_eq!(document.assets[0].alt, "The cover");
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(
        document.blocks[1].kind,
        BlockKind::Image {
            asset_id: document.assets[0].id.clone(),
        }
    );
}

#[test]
fn an_image_wrapped_in_its_own_paragraph_keeps_document_order() {
    let document = parse_html(
        r#"<p>Before.</p><p><img src="https://reader.example/figure.png" alt="Figure"></p><p>After.</p>"#,
        &HtmlParseOptions::default(),
    )
    .expect("paragraph-wrapped image");

    assert_eq!(document.blocks.len(), 3);
    assert_eq!(document.blocks[0].text, "Before.");
    assert!(matches!(document.blocks[1].kind, BlockKind::Image { .. }));
    assert_eq!(document.blocks[1].text, "Figure");
    assert_eq!(document.blocks[2].text, "After.");
}

#[test]
fn html_rejects_input_larger_than_the_explicit_parse_budget() {
    let html = "<p>This input is deliberately larger than sixteen bytes.</p>";
    let options = HtmlParseOptions {
        max_input_bytes: 16,
        ..HtmlParseOptions::default()
    };

    let error = parse_html(html, &options).expect_err("input should exceed its budget");

    assert_eq!(
        error,
        DocumentError::InputTooLarge {
            actual: html.len(),
            limit: 16,
        }
    );
}

#[test]
fn html_stops_when_semantic_block_budget_is_exhausted() {
    let html = "<p>First.</p><p>Second.</p>";
    let options = HtmlParseOptions {
        max_blocks: 1,
        ..HtmlParseOptions::default()
    };

    let error = parse_html(html, &options).expect_err("two blocks exceed the budget");

    assert_eq!(error, DocumentError::TooManyBlocks { limit: 1 });
}

#[test]
fn unrelated_content_inserted_before_a_block_does_not_change_its_id() {
    let original = parse_html("<p>The stable paragraph.</p>", &HtmlParseOptions::default())
        .expect("original HTML");
    let edited = parse_html(
        "<p>A newly inserted introduction.</p><p>The stable paragraph.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("edited HTML");

    assert_eq!(original.blocks[0].id, edited.blocks[1].id);
}

#[test]
fn repeated_identical_blocks_receive_distinct_but_deterministic_ids() {
    let html = "<p>Repeated.</p><p>Repeated.</p>";

    let first = parse_html(html, &HtmlParseOptions::default()).expect("first parse");
    let second = parse_html(html, &HtmlParseOptions::default()).expect("second parse");

    assert_ne!(first.blocks[0].id, first.blocks[1].id);
    assert_eq!(first.blocks[0].id, second.blocks[0].id);
    assert_eq!(first.blocks[1].id, second.blocks[1].id);
}
