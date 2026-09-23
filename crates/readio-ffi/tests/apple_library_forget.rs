#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_library_names_the_file_consequence_before_forgetting_an_item() {
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
        "readio-apple-library-forget-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("Swift fixture directory");
    let fixture = fixture_dir.join("AppleLibraryForget.swift");
    let executable = fixture_dir.join("apple-library-forget");
    fs::write(&fixture, SWIFT_FIXTURE).expect("Swift fixture source");

    let result = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(manifest_dir.join("include"))
        .arg(apple.join("Core/ReadioSDK.swift"))
        .arg(apple.join("Views/Common/AppleLocalization.swift"))
        .arg(apple.join("Views/Library/LibraryForgetPresentation.swift"))
        .arg(&fixture)
        .arg(deps_dir.join("libreadio_ffi.a"))
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple library copy");
    if !result.status.success() {
        let _ = fs::remove_dir_all(&fixture_dir);
        panic!(
            "Apple forget copy must compile:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let run = Command::new(&executable)
        .output()
        .expect("Apple forget fixture can run");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(
        run.status.success(),
        "Apple forget copy must name linked and owned consequences:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@main
enum AppleLibraryForgetSmoke {
    static func main() {
        let linked = LibraryForgetPresentation.resolve(
            itemTitle: "Linked Notes",
            ownership: .link
        )
        guard linked.title == "Remove from Library?",
              linked.message.contains("linked source file stays"),
              linked.actionLabel == "Remove" else {
            fatalError("linked source ownership was not explained")
        }

        let copied = LibraryForgetPresentation.resolve(
            itemTitle: "Owned Book",
            ownership: .copy
        )
        guard copied.title == "Delete Readio Copy?",
              copied.message.contains("Readio-managed copy"),
              copied.message.contains("original source is unaffected"),
              copied.actionLabel == "Delete" else {
            fatalError("owned copy deletion was not explained")
        }

        let legacy = LibraryForgetPresentation.resolve(
            itemTitle: "Legacy Book",
            ownership: nil
        )
        guard legacy.message.contains("if present") else {
            fatalError("legacy ownership made an unsafe claim")
        }
    }
}
"#;
