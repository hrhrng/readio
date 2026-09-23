#![cfg(target_vendor = "apple")]

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const EPUB_FIGURE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\napple-figure";

#[test]
fn apple_library_and_settings_services_work_without_an_http_server() {
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
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-local-services-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleLocalServicesSmoke.swift");
    let executable = fixture_dir.join("apple-local-services-smoke");
    let pdf_fixture = fixture_dir.join("readable.pdf");
    let epub_fixture = fixture_dir.join("spine-order.epub");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");
    fs::write(
        &pdf_fixture,
        minimal_text_pdf(&[
            "A readable PDF sentence with enough text for extraction.",
            "A second paragraph remains available offline.",
        ]),
    )
    .expect("the PDF fixture can be written");
    write_epub(&epub_fixture);

    let sources = [
        "Models/AppSettings.swift",
        "Models/LibraryItem.swift",
        "Models/LibraryItemSummary.swift",
        "Models/LibraryItemsResponse.swift",
        "Models/Sentence.swift",
        "Models/ContentBlock.swift",
        "Models/EpubChapter.swift",
        "Models/ReaderProjection.swift",
        "Models/RustEPUBPublication.swift",
        "Models/RustPDFPublication.swift",
        "Networking/APIEndpoints.swift",
        "Networking/APIClient.swift",
        "Core/ReadioSDK.swift",
        "Core/AppleDiagnostics.swift",
        "Core/AppleReplicaSync.swift",
        "Core/AppleLocalReplicaStore.swift",
        "Core/AppleReadingPositionService.swift",
        "Core/AppleBookmarkService.swift",
        "Networking/LibraryService.swift",
        "Networking/SettingsService.swift",
        "ViewModels/ReaderViewModel.swift",
    ];
    let mut command = Command::new("swiftc");
    command
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(&include_dir);
    for source in sources {
        command.arg(apple.join(source));
    }
    let compile = command
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple local services");
    assert!(
        compile.status.success(),
        "Apple services must compile against the local Rust store:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .env("READIO_TEST_PDF", &pdf_fixture)
        .env("READIO_TEST_EPUB", &epub_fixture)
        .output()
        .expect("the Apple local services fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple services must provide offline CRUD without HTTP:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r##"
import Foundation

struct FixtureURLAcquirer: AppleURLAcquiring {
    func acquire(url: URL) async throws -> AppleAcquiredURLResource {
        AppleAcquiredURLResource(
            finalURL: url,
            data: Data(
                "<html><head><title>Ignored chrome</title></head><body><nav>Menu</nav><main><h1>Rust Article</h1><p>First paragraph.</p><script>steal()</script><p>Second paragraph.</p></main></body></html>".utf8
            ),
            mediaType: "text/html"
        )
    }
}

final class FixtureOperationIDs: @unchecked Sendable {
    private let lock = NSLock()
    private var value = 0

    func next() -> String {
        lock.lock()
        defer { lock.unlock() }
        value += 1
        return "op-apple-import-\(value)"
    }
}

@main
enum AppleLocalServicesSmoke {
    @MainActor
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-local-services-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let store = AppleLocalReplicaStore(
            statePersistence: FileAppleReplicaSyncPersistence(
                fileURL: root.appendingPathComponent("replica.json")
            ),
            contentPersistence: FileAppleLibraryContentPersistence(
                rootDirectory: root.appendingPathComponent("content")
            )
        )
        let operationIDs = FixtureOperationIDs()
        let diagnostics = AppleDiagnosticLog(
            now: { 375 },
            makeOperationID: { operationIDs.next() }
        )
        let library = LibraryService(
            localStore: store,
            urlAcquirer: FixtureURLAcquirer(),
            languageTag: "zh-Hans-CN",
            diagnostics: diagnostics
        )
        let settings = SettingsService(localStore: store)

        let sample = try await library.fetchItem(id: "readio:sample:v1")
        guard sample.id == "readio:sample:v1",
              sample.legacyItemIds == ["6b4121313cc4a35d"],
              sample.locale == "zh",
              sample.title == "注意力的形状",
              sample.author == "readio 示例",
              sample.source == "readio://sample",
              sample.isBuiltinSample == true,
              !sample.content.isEmpty,
              !(sample.markdownSource ?? "").isEmpty,
              try await library.fetchItems().total == 0 else {
            throw FixtureError.builtinSampleWasNotVirtual
        }
        let sampleProjection = try await library.loadReaderProjection(for: sample)
        guard case .heading = sampleProjection.chapters[0].blocks[0],
              sampleProjection.chapters[0].blocks.contains(where: { block in
                  if case .blockquote = block { return true }
                  return false
              }) else {
            throw FixtureError.builtinSampleDidNotUseMarkdownReaderIR
        }

        let markdown = try await library.importFile(
            data: Data(
                "# Native Markdown\n\nRead **one thing** well.\n\n> A shared quote.".utf8
            ),
            filename: "native.md"
        )
        let markdownProjection = try await library.loadReaderProjection(for: markdown)
        guard markdown.contentFormat == "markdown",
              case .heading = markdownProjection.chapters[0].blocks[0],
              case let .paragraph(markdownSentences, _) = markdownProjection.chapters[0].blocks[1],
              markdownSentences[0].text == "Read one thing well.",
              markdownSentences[0].runs.map({ "\($0.text)|\($0.bold)|\($0.italic)" }) == [
                  "Read |false|false",
                  "one thing|true|false",
                  " well.|false|false",
              ],
              case .blockquote = markdownProjection.chapters[0].blocks[2] else {
            throw FixtureError.markdownImportDidNotUseSharedReaderIR
        }
        try await library.deleteItem(id: markdown.id)

        let imported = try await library.importFile(
            data: Data("One. Two.".utf8),
            filename: "offline.txt",
            title: "Offline Book"
        )
        guard imported.content == "One. Two.", imported.title == "Offline Book" else {
            throw FixtureError.importFailed
        }
        let pdfURL = URL(
            fileURLWithPath: ProcessInfo.processInfo.environment["READIO_TEST_PDF"]!
        )
        let pdf = try await library.importFile(
            data: Data(contentsOf: pdfURL),
            filename: "readable.pdf"
        )
        let projectedPDF = try RustPDFPublication.decode(pdf.content)
            .projectedForApple()
        guard pdf.type == .pdf,
              pdf.contentFormat == "readio-pdf-v1",
              pdf.content.contains("A readable PDF sentence"),
              !projectedPDF.chapters.isEmpty,
              try await library.fetchItem(id: pdf.id).content == pdf.content else {
            throw FixtureError.rustPDFImportFailed
        }
        let diagnosticEvents = diagnostics.snapshot()
        let markdownParsing = diagnosticEvents.first(where: { event in
            event.category == .parsing
                && event.fields["document_format"] == .string("markdown")
        })
        let markdownImport = diagnosticEvents.first(where: { event in
            event.category == .import
                && event.fields["document_format"] == .string("markdown")
                && event.fields["duplicate"] == .boolean(false)
        })
        let textParsing = diagnosticEvents.first(where: { event in
            event.category == .parsing
                && event.fields["document_format"] == .string("plain_text")
        })
        let textImport = diagnosticEvents.first(where: { event in
            event.category == .import
                && event.fields["document_format"] == .string("plain_text")
                && event.fields["duplicate"] == .boolean(false)
        })
        let pdfParsing = diagnosticEvents.first(where: { event in
            event.category == .parsing
                && event.fields["document_format"] == .string("pdf")
                && event.fields["chapter_count"] == .number(1)
        })
        let pdfImport = diagnosticEvents.first(where: { event in
            event.category == .import
                && event.fields["document_format"] == .string("pdf")
                && event.fields["source_kind"] == .string("file")
                && event.fields["duplicate"] == .boolean(false)
        })
        guard markdownParsing?.operationID == markdownImport?.operationID,
              markdownParsing != nil,
              textParsing?.operationID == textImport?.operationID,
              textParsing != nil,
              pdfParsing?.operationID == pdfImport?.operationID,
              pdfParsing != nil else {
            throw FixtureError.missingDiagnostics
        }
        let epubURL = URL(
            fileURLWithPath: ProcessInfo.processInfo.environment["READIO_TEST_EPUB"]!
        )
        let epub = try await library.importFile(
            data: Data(contentsOf: epubURL),
            filename: "fallback-name.epub"
        )
        let epubPublication = try RustEPUBPublication.decode(epub.content)
        let epubProjection = epubPublication.projectedForApple()
        let chapters = epubProjection.chapters
        guard epub.type == .epub,
              epub.contentFormat == "readio-epub-v1",
              epub.title == "Spine Order",
              chapters.map(\.title) == ["Second Chapter", "First Chapter"],
              chapters.allSatisfy({ !$0.blocks.isEmpty }),
              epub.content.contains("Second body is readable."),
              epub.content.contains("First body is readable."),
              !epub.content.contains("This note is outside reading order."),
              try await library.fetchItem(id: epub.id).content == epub.content else {
            throw FixtureError.rustEPUBImportFailed
        }
        guard case let .paragraph(secondBody, _) = chapters[0].blocks[1],
              secondBody[0].runs.map({ "\($0.text)|\($0.bold)|\($0.italic)" }) == [
                "Second |false|false",
                "body|true|false",
                " is |false|false",
                "readable|false|true",
                ".|false|false",
              ] else {
            throw FixtureError.rustEPUBEmphasisWasFlattened
        }
        guard case let .image(src, assetID, mediaType, alt) = chapters[0].blocks[2],
              src == nil,
              let assetID,
              mediaType == "image/png",
              alt == "Architecture diagram",
              epub.coverImage == "readio-epub-asset:\(assetID)",
              try await library.loadEpubAsset(itemID: epub.id, assetID: assetID)
                == Data([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] + Array("apple-figure".utf8)) else {
            throw FixtureError.rustEPUBAssetWasNotResolved
        }
        let epubReader = ReaderViewModel(
            libraryService: library,
            positionService: AppleReadingPositionService(localStore: store),
            bookmarkService: AppleBookmarkService(localStore: store),
            sessionID: "epub-reader"
        )
        await epubReader.loadItem(id: epub.id)
        epubReader.search("body")
        guard epubReader.searchResult?.totalOccurrences == 2,
              epubReader.searchHits.count == 2,
              epubReader.currentChapterId == chapters[0].id,
              epubReader.sentences.map(\.charOffset) == [0, 15],
              epubReader.sentences.map(\.readerIndex) == [0, 1],
              epubReader.activateSearchHit(at: 1) == 1,
              epubReader.currentChapterId == chapters[1].id,
              epubReader.sentences.map(\.charOffset) == [61, 75],
              epubReader.sentences.map(\.readerIndex) == [2, 3] else {
            throw FixtureError.epubSearchDidNotUseOneWholeBookCoordinate
        }
        guard epubReader.tocEntries.map(\.title) == [
            "Part Two",
            "Focused body",
            "Part One",
        ] else {
            throw FixtureError.epubNavigationWasFlattenedToSpineTitles
        }
        await epubReader.selectChapter(id: epubReader.tocEntries[1].id)
        guard epubReader.currentChapterId == chapters[0].id,
              epubReader.restoredSentenceIndex == 1 else {
            throw FixtureError.epubNavigationDidNotResolveBlockTarget
        }
        _ = epubReader.activateSearchHit(at: 1)
        await epubReader.addBookmark(sentenceIndex: 1)
        guard let secondChapterBookmark = epubReader.bookmarks.first else {
            throw FixtureError.structuredBookmarkDidNotSwitchChapter
        }
        await epubReader.saveProgress(sentenceIndex: 1)
        await epubReader.selectChapter(id: chapters[0].id)
        guard epubReader.activateBookmark(secondChapterBookmark) == 1,
              epubReader.currentChapterId == chapters[1].id else {
            throw FixtureError.structuredBookmarkDidNotSwitchChapter
        }
        await epubReader.selectChapter(id: chapters[0].id)
        await epubReader.refreshReplicaProjection()
        guard epubReader.currentChapterId == chapters[1].id,
              epubReader.restoredSentenceIndex == 1 else {
            throw FixtureError.syncedStructuredPositionDidNotSwitchChapter
        }
        let page = try await library.fetchItems(
            search: "offline",
            page: 1,
            pageSize: 20,
            sortBy: "title",
            sortOrder: "asc"
        )
        guard page.total == 1, page.items.first?.id == imported.id else {
            throw FixtureError.queryFailed
        }
        let detail = try await library.fetchItem(id: imported.id)
        guard detail.content == "One. Two." else {
            throw FixtureError.detailFailed
        }

        let web = try await library.importURL(url: "https://example.com/article")
        let webReader = try JSONDecoder().decode(
            ReadioReaderIR.self,
            from: Data(web.content.utf8)
        )
        guard web.title == "Rust Article",
              web.contentFormat == "readio-reader-ir-v1",
              webReader.document.blocks.map(\.text) == [
                  "Rust Article",
                  "First paragraph.",
                  "Second paragraph.",
              ],
              try await library.fetchItem(id: web.id).content == web.content else {
            throw FixtureError.rustHTMLImportFailed
        }
        try await library.updateProgress(id: web.id, progress: 71)
        let duplicateWeb = try await library.importURL(
            url: "https://example.com/article",
            title: "Replacement web title"
        )
        guard duplicateWeb.id == web.id,
              duplicateWeb.title == "Rust Article",
              duplicateWeb.progress == 71,
              try await library.fetchItems().total == 4 else {
            throw FixtureError.duplicateURLImportDidNotReuseProgress
        }
        let webDiagnosticEvents = diagnostics.snapshot()
        let htmlParsing = webDiagnosticEvents.first(where: { event in
            event.category == .parsing
                && event.fields["document_format"] == .string("html")
        })
        let htmlImport = webDiagnosticEvents.first(where: { event in
            event.category == .import
                && event.fields["document_format"] == .string("html")
                && event.fields["source_kind"] == .string("url")
                && event.fields["duplicate"] == .boolean(false)
        })
        guard htmlParsing?.operationID == htmlImport?.operationID,
              htmlParsing != nil else {
            throw FixtureError.missingURLDiagnostics
        }

        try await library.updateProgress(id: imported.id, progress: 42)
        try await library.updateVoice(id: imported.id, voice: "native-en")
        try await library.updateSpeed(id: imported.id, speed: 1.5)
        try await library.updateChapter(id: imported.id, chapter: "chapter-2")
        let updated = try await library.fetchItem(id: imported.id)
        guard updated.progress == 42,
              updated.voice == "native-en",
              updated.speed == 1.5,
              updated.currentChapter == "chapter-2" else {
            throw FixtureError.patchFailed
        }

        let duplicate = try await library.importFile(
            data: Data("One. Two.".utf8),
            filename: "renamed.txt",
            title: "Replacement title"
        )
        guard duplicate.id == imported.id,
              duplicate.title == "Offline Book",
              duplicate.progress == 42,
              try await library.fetchItems().total == 4 else {
            throw FixtureError.duplicateImportDidNotReuseProgress
        }

        let linkedMetadata = """
        {"id":"linked-book","title":"Linked book","type":"txt","progress":0,"date":"2026-08-07T00:00:00Z","category":"imported","folder_id":"f1","source":"/outside.txt","file_path":"/outside.txt","cover_image":null,"voice":null,"speed":null,"current_chapter":null,"content_format":"plaintext","import_record":{"item_id":"linked-book","content_identity":{"algorithm":"blake3-prefix-v1","byte_length":13,"digest":"linked-content"},"ownership":"link","source_key":"/outside.txt","storage_key":"/outside.txt"}}
        """
        try await store.saveLibraryItem(
            metadataJSON: linkedMetadata,
            content: AppleLibraryContentRecord(
                itemId: "linked-book",
                content: "Outside bytes.",
                fileData: nil,
                mediaType: "text/plain",
                fileName: "outside.txt"
            )
        )
        try await library.deleteItem(id: "linked-book")
        guard try await store.libraryContent(itemID: "linked-book")?.content == "Outside bytes." else {
            throw FixtureError.linkedSourceWasDeleted
        }

        try await settings.patchSettings(data: ["theme": "dark", "default_speed": "1.5"])
        guard try await settings.fetchSettings() == [
            "theme": "dark",
            "default_speed": "1.5",
        ] else {
            throw FixtureError.settingsFailed
        }

        try await library.deleteItem(id: imported.id)
        guard try await library.fetchItems().total == 3 else {
            throw FixtureError.deleteFailed
        }
    }
}

enum FixtureError: Error {
    case builtinSampleWasNotVirtual
    case builtinSampleDidNotUseMarkdownReaderIR
    case markdownImportDidNotUseSharedReaderIR
    case importFailed
    case queryFailed
    case detailFailed
    case rustHTMLImportFailed
    case rustPDFImportFailed
    case missingDiagnostics
    case missingURLDiagnostics
    case rustEPUBImportFailed
    case rustEPUBEmphasisWasFlattened
    case rustEPUBAssetWasNotResolved
    case epubSearchDidNotUseOneWholeBookCoordinate
    case epubNavigationWasFlattenedToSpineTitles
    case epubNavigationDidNotResolveBlockTarget
    case structuredBookmarkDidNotSwitchChapter
    case syncedStructuredPositionDidNotSwitchChapter
    case patchFailed
    case duplicateImportDidNotReuseProgress
    case duplicateURLImportDidNotReuseProgress
    case linkedSourceWasDeleted
    case settingsFailed
    case deleteFailed
}
"##;

fn write_epub(path: &Path) {
    let file = fs::File::create(path).expect("the EPUB fixture can be created");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let entries = [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0"?>
            <container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles><rootfile full-path="OEBPS/content.opf" /></rootfiles>
            </container>"#,
        ),
        (
            "OEBPS/content.opf",
            r#"<?xml version="1.0"?>
            <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>Spine Order</dc:title>
                <dc:creator>Rust Reader</dc:creator>
              </metadata>
              <manifest>
                <item id="first" href="first.xhtml" media-type="application/xhtml+xml" />
                <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml" />
                <item id="second" href="second.xhtml" media-type="application/xhtml+xml" />
                <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav" />
                <item id="figure" href="figure.png" media-type="image/png" properties="cover-image" />
              </manifest>
              <spine>
                <itemref idref="second" />
                <itemref idref="notes" linear="no" />
                <itemref idref="first" />
              </spine>
            </package>"#,
        ),
        (
            "OEBPS/first.xhtml",
            "<html><body><h1>First Chapter</h1><p>First body is readable.</p></body></html>",
        ),
        (
            "OEBPS/notes.xhtml",
            "<html><body><h1>Notes</h1><p>This note is outside reading order.</p></body></html>",
        ),
        (
            "OEBPS/second.xhtml",
            "<html><body><h1>Second Chapter</h1><p id=\"body\">Second <strong>body</strong> is <em>readable</em>.</p><img src=\"figure.png\" alt=\"Architecture diagram\"></body></html>",
        ),
        (
            "OEBPS/nav.xhtml",
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
              <body><nav epub:type="toc"><ol>
                <li><a href="second.xhtml">Part Two</a><ol>
                  <li><a href="second.xhtml#body">Focused body</a></li>
                </ol></li>
                <li><a href="first.xhtml">Part One</a></li>
              </ol></nav></body>
            </html>"#,
        ),
    ];
    for (name, contents) in entries {
        archive
            .start_file(name, options)
            .expect("the EPUB fixture entry can be started");
        archive
            .write_all(contents.as_bytes())
            .expect("the EPUB fixture entry can be written");
    }
    archive
        .start_file("OEBPS/figure.png", options)
        .expect("the EPUB figure entry can be started");
    archive
        .write_all(EPUB_FIGURE_BYTES)
        .expect("the EPUB figure entry can be written");
    archive.finish().expect("the EPUB fixture can be finalized");
}

fn minimal_text_pdf(lines: &[&str]) -> Vec<u8> {
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
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [5 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        b"<< /Title (Readable PDF) >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 6 0 R >>".to_vec(),
    ];
    let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
    content.extend_from_slice(&stream);
    content.extend_from_slice(b"\nendstream");
    objects.push(content);

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
