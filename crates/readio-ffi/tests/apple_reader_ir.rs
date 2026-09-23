#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_gui_projects_the_parser_owned_reader_ir() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-reader-ir-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleReaderIRSmoke.swift");
    let executable = fixture_dir.join("apple-reader-ir-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple.join("Core/ReadioSDK.swift"))
        .arg(apple.join("Models/Sentence.swift"))
        .arg(apple.join("Models/ContentBlock.swift"))
        .arg(apple.join("Models/EpubChapter.swift"))
        .arg(apple.join("Models/ReaderProjection.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple Reader IR projection");
    assert!(
        compile.status.success(),
        "Apple GUI projection must compile against typed Reader IR:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple Reader IR fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple GUI must project Rust ranges without re-parsing text:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r##"
import Foundation

@main
enum AppleReaderIRSmoke {
    static func main() throws {
        let reader = try ReadioSDK().parseHTMLReader(
            "<h1 id='top'>Title</h1><p>Hello <strong>bold</strong>. Next!</p><pre>skip()</pre><img src='https://cdn.example/figure.png' alt='Figure'>"
        )
        let projection = reader.projectedForApple()

        guard projection.chapters.count == 1,
              projection.chapters[0].title == "Title",
              projection.navigation.map(\.title) == ["Title"],
              projection.sentences(in: "document").map(\.text) == [
                  "Title",
                  "Hello bold.",
                  "Next!",
              ],
              projection.sentences(in: "document").map(\.charOffset) == [0, 6, 18],
              projection.sentences(in: "document").map(\.readerIndex) == [0, 1, 2]
        else {
            throw FixtureError.readerRangesWereRecomputed
        }

        let blocks = projection.chapters[0].blocks
        guard blocks.count == 4,
              case let .paragraph(sentences, anchorID) = blocks[1],
              anchorID == reader.document.blocks[1].id,
              sentences.map(\.text) == ["Hello bold.", "Next!"],
              sentences[0].runs.map({ "\($0.text)|\($0.bold)|\($0.italic)" }) == [
                  "Hello |false|false",
                  "bold|true|false",
                  ".|false|false",
              ],
              case let .code(code, _) = blocks[2],
              code == "skip()",
              case let .image(source, assetID, _, alt) = blocks[3],
              source == "https://cdn.example/figure.png",
              assetID == nil,
              alt == "Figure"
        else {
            throw FixtureError.semanticBlocksWereFlattened
        }

        let markdown = try ReadioSDK().parseMarkdownReader(
            "# Native Markdown\n\nRead **one thing** well.\n\n> A shared quote.\n\n```rs\nlet calm = true;\n```"
        ).projectedForApple()
        guard markdown.navigation.map(\.title) == ["Native Markdown"],
              case let .paragraph(markdownSentences, _) = markdown.chapters[0].blocks[1],
              markdownSentences[0].runs.map({ "\($0.text)|\($0.bold)|\($0.italic)" }) == [
                  "Read |false|false",
                  "one thing|true|false",
                  " well.|false|false",
              ],
              case let .blockquote(quote, _) = markdown.chapters[0].blocks[2],
              quote.map(\.text) == ["A shared quote."],
              case let .code(code, _) = markdown.chapters[0].blocks[3],
              code == "let calm = true;"
        else {
            throw FixtureError.markdownWasReparsedOrFlattened
        }
    }
}

enum FixtureError: Error {
    case readerRangesWereRecomputed
    case semanticBlocksWereFlattened
    case markdownWasReparsedOrFlattened
}
"##;
