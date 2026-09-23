#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_diagnostics_exports_only_rust_redacted_recent_facts() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple_core = repo_root.join("apps/ios/Readio/Readio/Core");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-diagnostics-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("diagnostics fixture directory");
    let fixture = fixture_dir.join("AppleDiagnosticsSmoke.swift");
    fs::write(&fixture, SWIFT_FIXTURE).expect("diagnostics fixture source");
    let executable = fixture_dir.join("apple-diagnostics-smoke");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleDiagnostics.swift"))
        .arg(apple_core.join("AppleSupportBundleDocument.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple diagnostics");
    assert!(
        compile.status.success(),
        "Apple diagnostics must compile against the real Rust support-bundle ABI:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple diagnostics fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple diagnostics must keep only recent facts and export Rust-redacted JSON:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation
import SwiftUI

@main
enum AppleDiagnosticsSmoke {
    static func main() async throws {
        let log = AppleDiagnosticLog(
            capacity: 3,
            now: { 1_000 },
            makeOperationID: { "op-generated" }
        )

        guard await log.beginOperation() == "op-generated" else {
            throw SmokeError.unexpectedOperationID
        }

        for index in 0..<4 {
            await log.record(
                operationID: "op-\(index)",
                component: "apple-player",
                category: .playback,
                name: "clock_sample",
                fields: [
                    "sentence_index": .number(Double(index)),
                    "media_position_ms": .number(Double(index * 250)),
                    "book_text": .string("A private sentence"),
                    "authorization": .string("Bearer private-token"),
                    "source_url": .string("https://private.example/book"),
                ]
            )
        }

        let snapshot = await log.snapshot()
        guard snapshot.map(\.operationID) == ["op-1", "op-2", "op-3"] else {
            throw SmokeError.unexpectedRing
        }

        let bundle = try await log.buildSupportBundle(
            appVersion: "0.2.0-beta.12",
            generatedAtUnixMilliseconds: 2_000
        )
        guard bundle.schemaVersion == 1,
              bundle.platform == "apple",
              bundle.appVersion == "0.2.0-beta.12",
              bundle.events.map(\.operationID) == ["op-1", "op-2", "op-3"],
              bundle.events.last?.fields["media_position_ms"] == .number(750),
              bundle.redaction.contentIncluded == false,
              bundle.redaction.secretsIncluded == false,
              bundle.redaction.droppedFieldCount == 9 else {
            throw SmokeError.unexpectedBundle
        }

        let exported = try String(
            decoding: JSONEncoder.readioSupportBundle.encode(bundle),
            as: UTF8.self
        )
        guard !exported.localizedCaseInsensitiveContains("private sentence"),
              !exported.localizedCaseInsensitiveContains("private-token"),
              !exported.localizedCaseInsensitiveContains("authorization"),
              !exported.localizedCaseInsensitiveContains("source_url"),
              !exported.localizedCaseInsensitiveContains("https://") else {
            throw SmokeError.privateDataEscaped
        }

        let file = try ReadioSupportBundleFile(bundle: bundle)
        guard file.filename == "readio-support-1970-01-01T00-00-02-000Z.json",
              file.mediaType == "application/json",
              String(decoding: file.data, as: UTF8.self).hasSuffix("\n"),
              String(decoding: file.data, as: UTF8.self).contains(
                  "\"content_included\" : false"
              ) else {
            throw SmokeError.unexpectedFile
        }

        let document = ReadioSupportBundleDocument(file: file)
        requireFileDocument(document)
        guard document.file == file else {
            throw SmokeError.unexpectedDocument
        }

        let directFile = try await log.buildSupportBundleFile(
            appVersion: "0.2.0-beta.12",
            generatedAtUnixMilliseconds: 2_000
        )
        guard directFile == file else {
            throw SmokeError.unexpectedExportAction
        }
    }

    private static func requireFileDocument<T: FileDocument>(_ document: T) {}
}

enum SmokeError: Error {
    case unexpectedOperationID
    case unexpectedRing
    case unexpectedBundle
    case privateDataEscaped
    case unexpectedFile
    case unexpectedDocument
    case unexpectedExportAction
}
"#;
