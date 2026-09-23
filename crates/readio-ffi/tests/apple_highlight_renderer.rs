#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_renderer_projects_the_rust_scalar_range_without_retokenizing() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let deps_dir = env::current_exe()
        .expect("test executable path")
        .parent()
        .expect("Cargo deps directory")
        .to_owned();
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-highlight-renderer-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let fixture = fixture_dir.join("AppleHighlightRendererSmoke.swift");
    let renderer = fixture_dir.join("SentenceTextView.swift");
    let executable = fixture_dir.join("apple-highlight-renderer-smoke");
    let renderer_source = fs::read_to_string(apple.join("Views/Reader/SentenceTextView.swift"))
        .expect("renderer source");
    fs::write(
        &renderer,
        renderer_source
            .split_once("\n#Preview")
            .map_or(renderer_source.as_str(), |(production, _)| production),
    )
    .expect("sanitized renderer source");
    fs::write(
        &fixture,
        r#"
import Foundation

@main
enum AppleHighlightRendererSmoke {
    static func main() throws {
        let range = ReadioScalarRange(start: 9, end: 10)
        let projection = SentenceTextView.projectScalarHighlight(
            "🙂 hello, 世界!",
            range: range
        )
        guard projection == ReadioSentenceTextProjection(
            before: "🙂 hello, ",
            highlighted: "世",
            after: "界!"
        ) else {
            throw SmokeError.unexpectedProjection
        }

        let styled = SentenceTextView.projectStyledText(
            "Hello bold.",
            runs: [
                StyledRun(text: "Hello "),
                StyledRun(text: "bold", bold: true),
                StyledRun(text: "."),
            ],
            highlightRange: ReadioScalarRange(start: 6, end: 10)
        )
        guard styled == [
            ReadioStyledTextSegment(
                text: "Hello ",
                bold: false,
                italic: false,
                highlighted: false
            ),
            ReadioStyledTextSegment(
                text: "bold",
                bold: true,
                italic: false,
                highlighted: true
            ),
            ReadioStyledTextSegment(
                text: ".",
                bold: false,
                italic: false,
                highlighted: false
            ),
        ] else {
            throw SmokeError.inlineStyleWasDiscarded
        }

        let sentenceView = SentenceTextView(
            sentence: Sentence(index: 7, text: "🙂 hello, 世界!"),
            isActive: true,
            wordRange: range,
            runs: nil,
            onTap: {}
        )
        guard containsSwiftUIID(sentenceView.body, expected: 7) else {
            throw SmokeError.missingSentenceScrollAnchor
        }
    }

    private static func containsSwiftUIID(
        _ value: Any,
        expected: Int,
        depth: Int = 0
    ) -> Bool {
        guard depth < 64 else { return false }
        let mirror = Mirror(reflecting: value)
        for child in mirror.children {
            if child.label == "id", child.value as? Int == expected {
                return true
            }
            if containsSwiftUIID(child.value, expected: expected, depth: depth + 1) {
                return true
            }
        }
        return false
    }
}

enum SmokeError: Error {
    case unexpectedProjection
    case inlineStyleWasDiscarded
    case missingSentenceScrollAnchor
}
"#,
    )
    .expect("fixture source");

    let result = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(manifest_dir.join("include"))
        .arg(apple.join("Models/Sentence.swift"))
        .arg(apple.join("Models/ContentBlock.swift"))
        .arg(apple.join("Core/ReadioSDK.swift"))
        .arg(apple.join("Views/Common/ReadioAppleTheme.swift"))
        .arg(&renderer)
        .arg(&fixture)
        .arg(deps_dir.join("libreadio_ffi.a"))
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple scalar renderer");
    assert!(
        result.status.success(),
        "the Apple renderer must consume typed Rust scalar ranges:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple renderer fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the scalar renderer returned the wrong visual slices:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
