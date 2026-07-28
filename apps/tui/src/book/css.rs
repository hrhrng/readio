//! Which CSS classes mean "emphasis".
//!
//! EPUBs produced by conversion tools rarely say `<em>`. They say
//! `<span class="calibre14">` and put `font-style: italic` in a stylesheet, so a
//! reader that only honours semantic tags loses every italic in the book.
//!
//! This is not a CSS engine and does not try to be one. It looks for rules whose
//! declarations lean or bolden text, and remembers the class names they apply to.
//! Anything it cannot understand it ignores, which is the right failure: text
//! that should have been italic is still text.

use std::collections::HashMap;

/// Class name → whether it is bold (rather than merely italic).
pub type Classes = HashMap<String, bool>;

/// Read a stylesheet and collect the classes that emphasise text.
pub fn scan(css: &str) -> Classes {
    let mut out = Classes::new();
    let stripped = strip_comments(css);
    for rule in stripped.split('}') {
        let Some((selectors, body)) = rule.split_once('{') else {
            continue;
        };
        let Some(strong) = weight(body) else {
            continue;
        };
        for class in classes_in(selectors) {
            // Bold wins over italic when a class claims both: it is the louder
            // of the two, and the renderer can only pick one modifier.
            let slot = out.entry(class).or_insert(strong);
            *slot = *slot || strong;
        }
    }
    out
}

/// Whether a declaration block emphasises, and how.
///
/// `font-style: italic` or `oblique` leans; `font-weight: bold`, `bolder` or 600
/// and up shouts. `normal` is how a stylesheet cancels an inherited style, and
/// must not be read as emphasis.
pub fn weight(body: &str) -> Option<bool> {
    let mut italic = false;
    let mut bold = false;
    for declaration in body.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        match property.as_str() {
            "font-style" => italic |= value.starts_with("italic") || value.starts_with("oblique"),
            "font-weight" => {
                bold |= value.starts_with("bold")
                    || value
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u32>().ok())
                        .is_some_and(|n| n >= 600);
            }
            // `font: italic 12px/1.4 serif` is legal and not worth parsing
            // properly; the shorthand's first token is enough.
            "font" => {
                italic |= value.split_whitespace().any(|word| word == "italic");
                bold |= value.split_whitespace().any(|word| word == "bold");
            }
            _ => {}
        }
    }
    (italic || bold).then_some(bold)
}

/// Class names named by a selector list, ignoring everything else about it.
///
/// `.a, span.b em` yields `a` and `b`: a descendant selector is a promise about
/// context this cannot check, and honouring the class anyway is the friendlier
/// mistake — the alternative is dropping the emphasis entirely.
fn classes_in(selectors: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in selectors.split([',', ' ', '>', '+', '~']) {
        let mut rest = part;
        while let Some(at) = rest.find('.') {
            let tail = &rest[at + 1..];
            let end = tail
                .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(tail.len());
            let name = &tail[..end];
            if !name.is_empty() {
                out.push(name.to_string());
            }
            rest = &tail[end..];
        }
    }
    out
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversion_tools_class_names_are_picked_up() {
        let classes = scan(
            ".calibre14 { font-style: italic }\n\
             .calibre15 { font-weight: bold; color: #000 }\n\
             p.body { margin: 0 }",
        );
        assert_eq!(classes.get("calibre14"), Some(&false));
        assert_eq!(classes.get("calibre15"), Some(&true));
        assert!(
            !classes.contains_key("body"),
            "a class that only sets margins emphasises nothing"
        );
    }

    #[test]
    fn normal_is_not_emphasis() {
        let classes = scan(".plain { font-style: normal; font-weight: normal }");
        assert!(classes.is_empty());
    }

    #[test]
    fn numeric_weights_count_from_six_hundred() {
        let classes = scan(".light { font-weight: 400 } .heavy { font-weight: 700 }");
        assert!(!classes.contains_key("light"));
        assert_eq!(classes.get("heavy"), Some(&true));
    }

    #[test]
    fn comments_and_selector_lists_do_not_confuse_it() {
        let classes = scan("/* .fake { font-style: italic } */ .a, span.b { font-style: italic }");
        assert!(!classes.contains_key("fake"));
        assert_eq!(classes.get("a"), Some(&false));
        assert_eq!(classes.get("b"), Some(&false));
    }

    #[test]
    fn bold_wins_when_a_class_claims_both() {
        let classes = scan(".both { font-style: italic; font-weight: bold }");
        assert_eq!(classes.get("both"), Some(&true));
    }
}
