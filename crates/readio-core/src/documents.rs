use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

pub const DOCUMENT_IR_VERSION: u32 = 1;
pub const HTML_PARSER_VERSION: &str = "html-ir-3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentIdentity {
    pub algorithm: String,
    pub byte_length: u64,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRevision {
    pub content_identity: ContentIdentity,
    pub ir_version: u32,
    pub parser_version: String,
    pub revision_digest: String,
    pub char_count: u64,
}

impl Default for DocumentRevision {
    fn default() -> Self {
        Self {
            content_identity: ContentIdentity {
                algorithm: "blake3-prefix-v1".to_string(),
                byte_length: 0,
                digest: blake3::hash(&[]).to_hex().to_string(),
            },
            ir_version: DOCUMENT_IR_VERSION,
            parser_version: HTML_PARSER_VERSION.to_string(),
            revision_digest: blake3::hash(&[]).to_hex().to_string(),
            char_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlParseOptions {
    pub max_input_bytes: usize,
    pub max_blocks: usize,
    pub base_url: Option<String>,
}

impl Default for HtmlParseOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 5 * 1024 * 1024,
            max_blocks: 100_000,
            base_url: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub ir_version: u32,
    pub revision: DocumentRevision,
    pub blocks: Vec<Block>,
    pub assets: Vec<MediaAsset>,
    #[serde(default)]
    pub anchors: Vec<DocumentAnchor>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            ir_version: DOCUMENT_IR_VERSION,
            revision: DocumentRevision::default(),
            blocks: Vec::new(),
            assets: Vec::new(),
            anchors: Vec::new(),
        }
    }
}

impl Document {
    /// Returns the canonical reader text. Offsets are Unicode scalar indexes in
    /// this stream, never UTF-8 byte indexes or platform UTF-16 indexes.
    pub fn canonical_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub kind: BlockKind,
    pub text: String,
    #[serde(default)]
    pub inline_spans: Vec<InlineSpan>,
}

/// A styled half-open range in the block's normalized Unicode-scalar stream.
/// Platform adapters turn these portable ranges into native attributed runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    pub range: [u64; 2],
    pub bold: bool,
    pub italic: bool,
}

/// Resolves an author-provided HTML fragment to a stable semantic block. The
/// source identifier is never used as a DOM selector without platform escaping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAnchor {
    pub source_id: String,
    pub block_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    Quote,
    Code,
    Image { asset_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: String,
    pub kind: MediaKind,
    pub source: String,
    pub alt: String,
}

/// A platform-neutral media reference resolved by a container parser before
/// the HTML document IR is finalized. EPUB uses this seam to replace relative
/// archive paths with stable asset IDs without exposing those paths as URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMediaAsset {
    pub id: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    InputTooLarge { actual: usize, limit: usize },
    TooManyBlocks { limit: usize },
}

pub fn parse_html(html: &str, options: &HtmlParseOptions) -> Result<Document, DocumentError> {
    parse_html_with_optional_media_resolver(html, options, None)
}

/// Parses HTML while delegating image-source resolution to its owning media
/// container. The resolver is synchronous and byte-free: it only binds an
/// authored `src` to stable metadata already indexed by the caller.
pub fn parse_html_with_media_resolver(
    html: &str,
    options: &HtmlParseOptions,
    resolver: &dyn Fn(&str) -> Option<ResolvedMediaAsset>,
) -> Result<Document, DocumentError> {
    parse_html_with_optional_media_resolver(html, options, Some(resolver))
}

fn parse_html_with_optional_media_resolver(
    html: &str,
    options: &HtmlParseOptions,
    resolver: Option<&dyn Fn(&str) -> Option<ResolvedMediaAsset>>,
) -> Result<Document, DocumentError> {
    if html.len() > options.max_input_bytes {
        return Err(DocumentError::InputTooLarge {
            actual: html.len(),
            limit: options.max_input_bytes,
        });
    }
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    let mut document = Document::default();
    collect_blocks(&dom.document, &mut document, options, &[], resolver)?;
    document.revision = build_revision(html.as_bytes(), HTML_PARSER_VERSION, &document);
    Ok(document)
}

/// Builds a shared Document IR from semantic blocks produced by a non-HTML
/// container parser. PDF uses this path so locators, highlights, search, and
/// reading progress see the same stable block IDs as every other surface.
pub(crate) fn document_from_semantic_blocks(
    source: &[u8],
    parser_version: &str,
    blocks: Vec<(BlockKind, String)>,
) -> Document {
    let mut document = Document::default();
    for (kind, text) in blocks {
        let id = unique_block_id(&document, &kind, &text);
        document.blocks.push(Block {
            id,
            kind,
            text,
            inline_spans: Vec::new(),
        });
    }
    document.revision = build_revision(source, parser_version, &document);
    document
}

fn build_revision(source: &[u8], parser_version: &str, document: &Document) -> DocumentRevision {
    let canonical_text = document.canonical_text();
    let content_digest = blake3::hash(source).to_hex().to_string();
    let mut revision_hasher = blake3::Hasher::new();
    revision_hasher.update(b"readio-document-revision-v1\0");
    revision_hasher.update(canonical_text.as_bytes());
    revision_hasher.update(b"\0");
    revision_hasher.update(
        serde_json::to_string(&(&document.blocks, &document.assets, &document.anchors))
            .expect("document IR is serializable")
            .as_bytes(),
    );

    DocumentRevision {
        content_identity: ContentIdentity {
            algorithm: "blake3-prefix-v1".to_string(),
            byte_length: source.len() as u64,
            digest: content_digest,
        },
        ir_version: DOCUMENT_IR_VERSION,
        parser_version: parser_version.to_string(),
        revision_digest: revision_hasher.finalize().to_hex().to_string(),
        char_count: canonical_text.chars().count() as u64,
    }
}

pub(crate) fn refresh_document_revision(
    source: &[u8],
    parser_version: &str,
    document: &mut Document,
) {
    document.revision = build_revision(source, parser_version, document);
}

fn collect_blocks(
    handle: &Handle,
    document: &mut Document,
    options: &HtmlParseOptions,
    inherited_anchors: &[String],
    resolver: Option<&dyn Fn(&str) -> Option<ResolvedMediaAsset>>,
) -> Result<bool, DocumentError> {
    let mut scoped_anchors = inherited_anchors.to_vec();
    if let NodeData::Element { name, attrs, .. } = &handle.data {
        let tag = name.local.as_ref();
        if is_skipped(tag) {
            return Ok(false);
        }
        if let Some(anchor) = element_anchor(attrs) {
            push_unique_anchor(&mut scoped_anchors, anchor);
        }
        if tag == "img" {
            let previous_block_count = document.blocks.len();
            collect_image(handle, document, options, resolver)?;
            if document.blocks.len() > previous_block_count {
                let block_id = document.blocks[previous_block_count].id.clone();
                record_document_anchors(document, &scoped_anchors, &block_id);
                return Ok(true);
            }
            return Ok(false);
        }
        if let Some(kind) = block_kind(tag) {
            collect_descendant_anchors(handle, &mut scoped_anchors);
            let (text, inline_spans) = if matches!(kind, BlockKind::Code) {
                let mut text = String::new();
                collect_text(handle, true, &mut text);
                (normalize_code(&text), Vec::new())
            } else {
                collect_normalized_inline_text(handle)
            };
            if !text.is_empty() {
                ensure_block_budget(document, options)?;
                let id = unique_block_id(document, &kind, &text);
                document.blocks.push(Block {
                    id: id.clone(),
                    kind,
                    text,
                    inline_spans,
                });
                record_document_anchors(document, &scoped_anchors, &id);
                return Ok(true);
            }
            // Publishers commonly wrap an illustration in an otherwise empty
            // paragraph. A block container must not swallow the nested image
            // merely because it has no textual run of its own.
            let first_nested_block = document.blocks.len();
            let mut emitted_nested = false;
            for child in handle.children.borrow().iter() {
                let child_anchors = if emitted_nested {
                    &[][..]
                } else {
                    scoped_anchors.as_slice()
                };
                emitted_nested |=
                    collect_blocks(child, document, options, child_anchors, resolver)?;
            }
            if document.blocks.len() > first_nested_block {
                let block_id = document.blocks[first_nested_block].id.clone();
                record_document_anchors(document, &scoped_anchors, &block_id);
            }
            return Ok(emitted_nested);
        }
    }

    let mut emitted = false;
    for child in handle.children.borrow().iter() {
        let child_anchors = if emitted {
            &[][..]
        } else {
            scoped_anchors.as_slice()
        };
        emitted |= collect_blocks(child, document, options, child_anchors, resolver)?;
    }
    Ok(emitted)
}

fn element_anchor(attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>) -> Option<String> {
    attrs
        .borrow()
        .iter()
        .find(|attribute| matches!(attribute.name.local.as_ref(), "id" | "name"))
        .map(|attribute| attribute.value.trim().to_string())
        .filter(|anchor| !anchor.is_empty() && anchor.chars().count() <= 1_024)
}

fn collect_descendant_anchors(handle: &Handle, anchors: &mut Vec<String>) {
    for child in handle.children.borrow().iter() {
        if let NodeData::Element { attrs, name, .. } = &child.data {
            if is_skipped(name.local.as_ref()) {
                continue;
            }
            if let Some(anchor) = element_anchor(attrs) {
                push_unique_anchor(anchors, anchor);
            }
        }
        collect_descendant_anchors(child, anchors);
    }
}

fn push_unique_anchor(anchors: &mut Vec<String>, anchor: String) {
    if !anchors.iter().any(|existing| existing == &anchor) {
        anchors.push(anchor);
    }
}

fn record_document_anchors(document: &mut Document, anchors: &[String], block_id: &str) {
    for source_id in anchors {
        if document
            .anchors
            .iter()
            .any(|anchor| anchor.source_id == *source_id)
        {
            continue;
        }
        document.anchors.push(DocumentAnchor {
            source_id: source_id.clone(),
            block_id: block_id.to_string(),
        });
    }
}

fn collect_image(
    handle: &Handle,
    document: &mut Document,
    options: &HtmlParseOptions,
    resolver: Option<&dyn Fn(&str) -> Option<ResolvedMediaAsset>>,
) -> Result<(), DocumentError> {
    let NodeData::Element { attrs, .. } = &handle.data else {
        return Ok(());
    };
    let attrs = attrs.borrow();
    let source = attrs
        .iter()
        .find(|attr| attr.name.local.as_ref() == "src")
        .map(|attr| attr.value.to_string());
    let Some(resolved) = source.and_then(|source| match resolver {
        Some(resolver) => resolver(&source),
        None => resolve_safe_url(&source, options).map(|source| ResolvedMediaAsset {
            id: format!("asset-{:06}", document.assets.len()),
            source,
        }),
    }) else {
        return Ok(());
    };
    let alt = attrs
        .iter()
        .find(|attr| attr.name.local.as_ref() == "alt")
        .map(|attr| normalize_text(attr.value.as_ref()))
        .unwrap_or_default();
    ensure_block_budget(document, options)?;
    let asset_id = resolved.id;
    if !document.assets.iter().any(|asset| asset.id == asset_id) {
        document.assets.push(MediaAsset {
            id: asset_id.clone(),
            kind: MediaKind::Image,
            source: resolved.source,
            alt: alt.clone(),
        });
    }
    let id = format!("block-{:06}", document.blocks.len());
    document.blocks.push(Block {
        id,
        kind: BlockKind::Image { asset_id },
        text: alt,
        inline_spans: Vec::new(),
    });
    Ok(())
}

fn ensure_block_budget(
    document: &Document,
    options: &HtmlParseOptions,
) -> Result<(), DocumentError> {
    if document.blocks.len() >= options.max_blocks {
        return Err(DocumentError::TooManyBlocks {
            limit: options.max_blocks,
        });
    }
    Ok(())
}

fn block_id(kind: &BlockKind, text: &str) -> String {
    let kind = match kind {
        BlockKind::Heading { level } => format!("heading-{level}"),
        BlockKind::Paragraph => "paragraph".to_string(),
        BlockKind::Quote => "quote".to_string(),
        BlockKind::Code => "code".to_string(),
        BlockKind::Image { asset_id } => format!("image-{asset_id}"),
    };
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    format!("block-{digest:x}")
}

fn unique_block_id(document: &Document, kind: &BlockKind, text: &str) -> String {
    let base = block_id(kind, text);
    let duplicate_prefix = format!("{base}-duplicate-");
    let occurrence = document
        .blocks
        .iter()
        .filter(|block| block.id == base || block.id.starts_with(&duplicate_prefix))
        .count();
    if occurrence == 0 {
        base
    } else {
        format!("{base}-duplicate-{occurrence}")
    }
}

fn resolve_safe_url(source: &str, options: &HtmlParseOptions) -> Option<String> {
    let url = match options.base_url.as_deref() {
        Some(base) => Url::parse(base).ok()?.join(source).ok()?,
        None => Url::parse(source).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn collect_text(handle: &Handle, preserve_whitespace: bool, output: &mut String) {
    match &handle.data {
        NodeData::Text { contents } => output.push_str(&contents.borrow()),
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if is_skipped(tag) {
                return;
            }
            if tag == "br" {
                output.push(if preserve_whitespace { '\n' } else { ' ' });
                return;
            }
            for child in handle.children.borrow().iter() {
                collect_text(child, preserve_whitespace, output);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                collect_text(child, preserve_whitespace, output);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InlineStyle {
    bold: bool,
    italic: bool,
}

#[derive(Debug)]
struct InlineFragment {
    text: String,
    style: InlineStyle,
}

fn collect_normalized_inline_text(handle: &Handle) -> (String, Vec<InlineSpan>) {
    let mut fragments = Vec::new();
    collect_inline_fragments(handle, InlineStyle::default(), &mut fragments);

    let mut text = String::new();
    let mut inline_spans: Vec<InlineSpan> = Vec::new();
    let mut scalar_offset = 0_u64;
    let mut pending_space: Option<InlineStyle> = None;

    for fragment in fragments {
        for character in fragment.text.chars() {
            if character.is_whitespace() {
                if scalar_offset > 0 {
                    pending_space = Some(fragment.style);
                }
                continue;
            }
            if let Some(space_style) = pending_space.take() {
                push_normalized_character(
                    &mut text,
                    &mut inline_spans,
                    &mut scalar_offset,
                    ' ',
                    space_style,
                );
            }
            push_normalized_character(
                &mut text,
                &mut inline_spans,
                &mut scalar_offset,
                character,
                fragment.style,
            );
        }
    }

    (text, inline_spans)
}

fn push_normalized_character(
    text: &mut String,
    inline_spans: &mut Vec<InlineSpan>,
    scalar_offset: &mut u64,
    character: char,
    style: InlineStyle,
) {
    text.push(character);
    if style.bold || style.italic {
        match inline_spans.last_mut() {
            Some(span)
                if span.range[1] == *scalar_offset
                    && span.bold == style.bold
                    && span.italic == style.italic =>
            {
                span.range[1] += 1;
            }
            _ => inline_spans.push(InlineSpan {
                range: [*scalar_offset, *scalar_offset + 1],
                bold: style.bold,
                italic: style.italic,
            }),
        }
    }
    *scalar_offset += 1;
}

fn collect_inline_fragments(
    handle: &Handle,
    inherited_style: InlineStyle,
    output: &mut Vec<InlineFragment>,
) {
    match &handle.data {
        NodeData::Text { contents } => {
            push_inline_fragment(output, contents.borrow().to_string(), inherited_style)
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if is_skipped(tag) {
                return;
            }
            if tag == "br" {
                push_inline_fragment(output, " ".to_string(), inherited_style);
                return;
            }
            let style = InlineStyle {
                bold: inherited_style.bold || matches!(tag, "b" | "strong"),
                italic: inherited_style.italic || matches!(tag, "i" | "em"),
            };
            for child in handle.children.borrow().iter() {
                collect_inline_fragments(child, style, output);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                collect_inline_fragments(child, inherited_style, output);
            }
        }
    }
}

fn push_inline_fragment(output: &mut Vec<InlineFragment>, text: String, style: InlineStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(fragment) = output.last_mut()
        && fragment.style == style
    {
        fragment.text.push_str(&text);
        return;
    }
    output.push(InlineFragment { text, style });
}

fn block_kind(tag: &str) -> Option<BlockKind> {
    match tag {
        "h1" => Some(BlockKind::Heading { level: 1 }),
        "h2" => Some(BlockKind::Heading { level: 2 }),
        "h3" => Some(BlockKind::Heading { level: 3 }),
        "h4" => Some(BlockKind::Heading { level: 4 }),
        "h5" => Some(BlockKind::Heading { level: 5 }),
        "h6" => Some(BlockKind::Heading { level: 6 }),
        "p" | "li" | "dd" | "dt" | "figcaption" => Some(BlockKind::Paragraph),
        "blockquote" => Some(BlockKind::Quote),
        "pre" => Some(BlockKind::Code),
        _ => None,
    }
}

fn is_skipped(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "script" | "style" | "template" | "svg" | "nav" | "footer" | "noscript"
    )
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_code(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}
