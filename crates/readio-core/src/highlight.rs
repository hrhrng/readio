use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::documents::{BlockKind, Document};

pub const TEXT_INDEX_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTextIndex {
    pub version: u16,
    pub document_revision: String,
    pub sentences: Vec<SentenceRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentenceRange {
    pub id: String,
    pub index: u32,
    pub block_id: String,
    /// Half-open Unicode-scalar range within the block.
    pub range: [u64; 2],
    /// The same range in the document's canonical scalar stream.
    pub char_range: [u64; 2],
    pub text: String,
    pub words: Vec<WordRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordRange {
    pub index: u32,
    pub range: [u64; 2],
    pub char_range: [u64; 2],
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightCursor {
    pub document_revision: String,
    pub sentence_index: u32,
    #[serde(default)]
    pub word_index: Option<u32>,
    #[serde(default)]
    pub word_progress_basis_points: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightTarget {
    pub sentence_id: String,
    pub sentence_index: u32,
    pub block_id: String,
    pub sentence_range: [u64; 2],
    pub word_range: Option<[u64; 2]>,
    pub sentence_char_range: [u64; 2],
    pub word_char_range: Option<[u64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightError {
    StaleRevision {
        expected: String,
        actual: String,
    },
    SentenceNotFound {
        sentence_index: u32,
    },
    WordNotFound {
        sentence_index: u32,
        word_index: u32,
    },
}

impl std::fmt::Display for HighlightError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "highlight revision is stale: requested {expected}, current {actual}"
            ),
            Self::SentenceNotFound { sentence_index } => {
                write!(
                    formatter,
                    "highlight sentence {sentence_index} does not exist"
                )
            }
            Self::WordNotFound {
                sentence_index,
                word_index,
            } => write!(
                formatter,
                "highlight word {word_index} does not exist in sentence {sentence_index}"
            ),
        }
    }
}

impl std::error::Error for HighlightError {}

pub fn index_document(document: &Document) -> DocumentTextIndex {
    let mut sentences = Vec::new();
    let mut block_start = 0_u64;
    for (block_index, block) in document.blocks.iter().enumerate() {
        if !matches!(block.kind, BlockKind::Code | BlockKind::Image { .. }) {
            for range in sentence_ranges(&block.text) {
                let text = scalar_slice(&block.text, range);
                let index = sentences.len() as u32;
                let char_range = [block_start + range[0], block_start + range[1]];
                sentences.push(SentenceRange {
                    id: sentence_id(&block.id, range, &text),
                    index,
                    block_id: block.id.clone(),
                    range,
                    char_range,
                    words: word_ranges(&text, range[0], block_start),
                    text,
                });
            }
        }
        block_start = block_start
            .saturating_add(block.text.chars().count() as u64)
            .saturating_add(u64::from(block_index + 1 < document.blocks.len()));
    }
    DocumentTextIndex {
        version: TEXT_INDEX_VERSION,
        document_revision: document.revision.revision_digest.clone(),
        sentences,
    }
}

pub fn resolve_highlight(
    document: &Document,
    index: &DocumentTextIndex,
    cursor: HighlightCursor,
) -> Result<HighlightTarget, HighlightError> {
    let actual = &document.revision.revision_digest;
    if cursor.document_revision != *actual || index.document_revision != *actual {
        return Err(HighlightError::StaleRevision {
            expected: cursor.document_revision,
            actual: actual.clone(),
        });
    }
    let sentence = index.sentences.get(cursor.sentence_index as usize).ok_or(
        HighlightError::SentenceNotFound {
            sentence_index: cursor.sentence_index,
        },
    )?;
    let requested_word = cursor.word_index.or_else(|| {
        let progress = cursor.word_progress_basis_points?;
        (!sentence.words.is_empty()).then(|| {
            ((usize::from(progress.min(10_000)) * sentence.words.len()) / 10_000)
                .min(sentence.words.len() - 1) as u32
        })
    });
    let word = match requested_word {
        Some(word_index) => Some(sentence.words.get(word_index as usize).ok_or(
            HighlightError::WordNotFound {
                sentence_index: cursor.sentence_index,
                word_index,
            },
        )?),
        None => None,
    };
    Ok(HighlightTarget {
        sentence_id: sentence.id.clone(),
        sentence_index: sentence.index,
        block_id: sentence.block_id.clone(),
        sentence_range: sentence.range,
        word_range: word.map(|word| word.range),
        sentence_char_range: sentence.char_range,
        word_char_range: word.map(|word| word.char_range),
    })
}

fn sentence_ranges(text: &str) -> Vec<[u64; 2]> {
    let scalars = text.chars().collect::<Vec<_>>();
    let mut ranges = Vec::new();
    let mut start = skip_whitespace(&scalars, 0);
    let mut cursor = start;
    while cursor < scalars.len() {
        let character = scalars[cursor];
        if is_sentence_terminal(character) {
            let mut end = cursor + 1;
            while end < scalars.len() && is_closing_punctuation(scalars[end]) {
                end += 1;
            }
            let is_boundary = is_cjk_terminal(character)
                || end == scalars.len()
                || scalars.get(end).is_some_and(|next| next.is_whitespace());
            if is_boundary {
                ranges.push([start as u64, end as u64]);
                start = skip_whitespace(&scalars, end);
                cursor = start;
                continue;
            }
        }
        cursor += 1;
    }
    if start < scalars.len() {
        ranges.push([start as u64, scalars.len() as u64]);
    }
    ranges
}

fn word_ranges(text: &str, sentence_start: u64, block_start: u64) -> Vec<WordRange> {
    text.unicode_word_indices()
        .enumerate()
        .map(|(index, (byte_start, word))| {
            let local_start = sentence_start + text[..byte_start].chars().count() as u64;
            let local_end = local_start + word.chars().count() as u64;
            WordRange {
                index: index as u32,
                range: [local_start, local_end],
                char_range: [block_start + local_start, block_start + local_end],
                text: word.to_string(),
            }
        })
        .collect()
}

fn sentence_id(block_id: &str, range: [u64; 2], text: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"readio-sentence-v1\0");
    hasher.update(block_id.as_bytes());
    hasher.update(&range[0].to_le_bytes());
    hasher.update(&range[1].to_le_bytes());
    hasher.update(text.as_bytes());
    format!("sentence-{}", hasher.finalize().to_hex())
}

fn scalar_slice(text: &str, range: [u64; 2]) -> String {
    text.chars()
        .skip(range[0] as usize)
        .take(range[1].saturating_sub(range[0]) as usize)
        .collect()
}

fn skip_whitespace(scalars: &[char], mut offset: usize) -> usize {
    while scalars
        .get(offset)
        .is_some_and(|character| character.is_whitespace())
    {
        offset += 1;
    }
    offset
}

fn is_sentence_terminal(character: char) -> bool {
    matches!(character, '.' | '!' | '?' | '。' | '！' | '？')
}

fn is_cjk_terminal(character: char) -> bool {
    matches!(character, '。' | '！' | '？')
}

fn is_closing_punctuation(character: char) -> bool {
    matches!(
        character,
        '"' | '\'' | '”' | '’' | '」' | '』' | '）' | ')' | '】' | ']'
    )
}
