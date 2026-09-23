#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_cover_view_recognizes_only_stable_local_epub_asset_references() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let cover_source = repo_root.join("apps/ios/Readio/Readio/Views/Common/CoverImageView.swift");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-epub-cover-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("cover fixture directory");
    let sanitized_cover = fixture_dir.join("CoverImageView.swift");
    let contents = fs::read_to_string(cover_source).expect("cover source");
    fs::write(
        &sanitized_cover,
        contents
            .split_once("\n#Preview")
            .map_or(contents.as_str(), |(production, _)| production),
    )
    .expect("sanitized cover source");
    let fixture = fixture_dir.join("CoverSmoke.swift");
    let executable = fixture_dir.join("cover-smoke");
    fs::write(
        &fixture,
        r#"
import SwiftUI

@main
enum CoverSmoke {
    static func main() {
        guard CoverImageView.epubAssetID(
            from: "readio-epub-asset:epub-asset-cover"
        ) == "epub-asset-cover",
        CoverImageView.epubAssetID(from: "https://example.com/cover.png") == nil,
        CoverImageView.epubAssetID(from: "readio-epub-asset:") == nil else {
            fatalError("cover references must never become arbitrary file paths")
        }
    }
}
"#,
    )
    .expect("cover fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(&sanitized_cover)
        .arg(&fixture)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-framework")
        .arg("ImageIO")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple cover references");
    assert!(
        compile.status.success(),
        "Apple cover reference adapter must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .output()
        .expect("cover fixture runs");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple cover reference adapter must reject non-asset sources:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
