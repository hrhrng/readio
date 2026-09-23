use serde::{Deserialize, Serialize};

pub const BUILTIN_SAMPLE_ITEM_ID: &str = "readio:sample:v1";
pub const BUILTIN_SAMPLE_SOURCE: &str = "readio://sample";
pub const LEGACY_TUI_SAMPLE_ITEM_ID: &str = "6b4121313cc4a35d";

const ENGLISH_MARKDOWN: &str = include_str!("sample_en.md");
const CHINESE_MARKDOWN: &str = include_str!("sample_zh.md");

/// A virtual document that ships with every Readio client and never enters the
/// user's imported-library namespace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuiltinSample {
    pub item_id: String,
    pub legacy_item_ids: Vec<String>,
    pub locale: String,
    pub title: String,
    pub author: String,
    pub source: String,
    pub content_format: String,
    pub content: String,
    pub markdown_source: String,
    pub is_builtin_sample: bool,
}

pub fn builtin_sample(language_tag: &str) -> BuiltinSample {
    let chinese = language_tag
        .trim()
        .split(['-', '_'])
        .next()
        .is_some_and(|language| language.eq_ignore_ascii_case("zh"));
    let (locale, title, author, markdown_source) = if chinese {
        ("zh", "注意力的形状", "readio 示例", CHINESE_MARKDOWN)
    } else {
        (
            "en",
            "The Shape of Attention",
            "readio sample",
            ENGLISH_MARKDOWN,
        )
    };

    BuiltinSample {
        item_id: BUILTIN_SAMPLE_ITEM_ID.to_string(),
        legacy_item_ids: vec![LEGACY_TUI_SAMPLE_ITEM_ID.to_string()],
        locale: locale.to_string(),
        title: title.to_string(),
        author: author.to_string(),
        source: BUILTIN_SAMPLE_SOURCE.to_string(),
        content_format: "plaintext".to_string(),
        content: canonical_sample_text(markdown_source),
        markdown_source: markdown_source.trim().to_string(),
        is_builtin_sample: true,
    }
}

fn canonical_sample_text(markdown: &str) -> String {
    markdown
        .lines()
        .map(|line| {
            let line = line.trim_end();
            if let Some(heading) = line.strip_prefix('#') {
                heading.trim_start_matches('#').trim_start()
            } else if let Some(quote) = line.strip_prefix('>') {
                quote.trim_start()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
