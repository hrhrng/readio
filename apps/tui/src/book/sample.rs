//! The virtual sample shipped by the shared Rust SDK.

use readio_sdk::ReadioSdk;

use super::{Book, Source, text};

pub fn definition() -> readio_sdk::samples::BuiltinSample {
    let language_tag = match crate::i18n::current() {
        crate::i18n::Lang::Zh => "zh",
        crate::i18n::Lang::En => "en",
    };
    ReadioSdk::new().samples.resolve(language_tag)
}

pub fn book() -> Book {
    let sample = definition();
    let chapters = text::parse_markdown(&sample.markdown_source, "attention.md")
        .expect("the embedded Markdown sample must remain valid Reader IR");
    Book {
        id: sample.item_id,
        title: sample.title,
        author: Some(sample.author),
        path: None,
        source: Source::Sample,
        chapters,
        cover: None,
    }
}
