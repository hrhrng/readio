#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_action_router_unifies_navigation_and_file_import_intent() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let router = repo_root.join("apps/ios/Readio/Readio/Views/Common/AppleAppRouter.swift");
    let localization =
        repo_root.join("apps/ios/Readio/Readio/Views/Common/AppleLocalization.swift");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-app-router-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("Apple router fixture directory");
    let fixture = fixture_dir.join("RouterSmoke.swift");
    let executable = fixture_dir.join("router-smoke");
    fs::write(
        &fixture,
        r#"
import Foundation

@main
enum RouterSmoke {
    @MainActor
    static func main() {
        let router = AppleAppRouter()
        guard router.destination == .home,
              router.readerItemID == nil,
              router.fileImportRequest == 0 else {
            fatalError("router must start at Home without a pending import")
        }

        router.openReader(itemID: "book-42")
        guard router.readerItemID == "book-42" else {
            fatalError("opening a book must enter the dedicated reader route")
        }
        router.closeReader()
        guard router.readerItemID == nil,
              router.destination == .home else {
            fatalError("closing the reader must restore the unchanged library shell route")
        }

        router.navigate(to: .library)
        guard router.destination == .library,
              router.readerItemID == nil,
              router.fileImportRequest == 0 else {
            fatalError("ordinary navigation must not manufacture an import request")
        }

        router.restoreDestination(rawValue: "settings")
        guard router.destination == .settings else {
            fatalError("a restored window must return to its last valid destination")
        }
        router.restoreDestination(rawValue: "removed-destination")
        guard router.destination == .home else {
            fatalError("stale scene state must fall back to Home")
        }

        router.requestFileImport()
        guard router.destination == .importItem,
              router.fileImportRequest == 1 else {
            fatalError("Import must navigate and emit one durable request")
        }
        guard router.consumeFileImportRequest(1),
              !router.consumeFileImportRequest(1) else {
            fatalError("one Import command must open exactly one file panel")
        }

        router.requestFileImport()
        guard router.fileImportRequest == 2,
              router.consumeFileImportRequest(2) else {
            fatalError("repeated Import commands must remain independently observable")
        }
    }
}
"#,
    )
    .expect("router fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(&localization)
        .arg(&router)
        .arg(&fixture)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple action router");
    assert!(
        compile.status.success(),
        "Apple action router must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("Apple action router fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple action router must preserve semantic intent:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
