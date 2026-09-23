#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn native_macos_uses_the_sidebar_split_root_when_size_class_is_absent() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let policy = repo_root.join("apps/ios/Readio/Readio/Views/Common/AppleRootLayoutPolicy.swift");
    let fixture_dir =
        env::temp_dir().join(format!("readio-native-macos-layout-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("macOS layout fixture directory");
    let fixture = fixture_dir.join("LayoutSmoke.swift");
    let executable = fixture_dir.join("layout-smoke");
    fs::write(
        &fixture,
        r#"
import SwiftUI

@main
enum LayoutSmoke {
    static func main() {
        guard AppleRootLayoutPolicy.presentation(horizontalSizeClass: nil) == .split else {
            fatalError("native macOS must not fall back to the compact tab layout")
        }
    }
}
"#,
    )
    .expect("layout fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(&policy)
        .arg(&fixture)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the native macOS layout policy");
    assert!(
        compile.status.success(),
        "native macOS layout policy must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("native macOS layout fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "native macOS must choose the sidebar split root:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
