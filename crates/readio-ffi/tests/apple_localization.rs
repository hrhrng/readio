#![cfg(target_vendor = "apple")]

use serde_json::Value;
use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_language_policy_resolves_chinese_variants_and_english_fallback() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-localization-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("Swift fixture directory");
    let fixture = fixture_dir.join("AppleLocalizationSmoke.swift");
    let executable = fixture_dir.join("apple-localization-smoke");
    let bundle = fixture_dir.join("Localization.bundle");
    fs::create_dir_all(bundle.join("en.lproj")).expect("English fixture catalog directory");
    fs::create_dir_all(bundle.join("zh-Hans.lproj"))
        .expect("Chinese fixture catalog directory");
    fs::copy(
        apple.join("Resources/en.lproj/Localizable.strings"),
        bundle.join("en.lproj/Localizable.strings"),
    )
    .expect("English fixture catalog");
    fs::copy(
        apple.join("Resources/zh-Hans.lproj/Localizable.strings"),
        bundle.join("zh-Hans.lproj/Localizable.strings"),
    )
    .expect("Chinese fixture catalog");
    fs::write(
        bundle.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleIdentifier</key><string>com.readio.localization-tests</string>
<key>CFBundleName</key><string>Localization</string>
<key>CFBundlePackageType</key><string>BNDL</string>
</dict></plist>"#,
    )
    .expect("fixture bundle plist");
    fs::write(&fixture, SWIFT_FIXTURE).expect("Swift fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(apple.join("Views/Common/AppleLocalization.swift"))
        .arg(apple.join("Views/Common/AppleAppRouter.swift"))
        .arg(&fixture)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple localization");
    assert!(
        compile.status.success(),
        "Apple language policy must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .env("READIO_LOCALIZATION_FIXTURE_BUNDLE", &bundle)
        .output()
        .expect("Apple localization fixture can run");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(
        run.status.success(),
        "Apple language policy must resolve supported locales:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn apple_catalogs_share_keys_and_translate_main_navigation() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let resources = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root")
        .join("apps/ios/Readio/Readio/Resources");
    let en = load_strings(resources.join("en.lproj/Localizable.strings"));
    let zh = load_strings(resources.join("zh-Hans.lproj/Localizable.strings"));
    let mut en_keys: Vec<_> = en.keys().collect();
    let mut zh_keys: Vec<_> = zh.keys().collect();
    en_keys.sort();
    zh_keys.sort();
    assert_eq!(zh_keys, en_keys, "Apple catalogs must register the same UI keys");
    assert_eq!(zh["Home"], "首页");
    assert_eq!(zh["Library"], "书库");
    assert_eq!(zh["Import"], "导入");
    assert_eq!(zh["Settings"], "设置");
    for key in [
        "library.book.singular",
        "library.book.plural",
        "library.inProgress",
        "library.removeLinked.message",
        "library.deleteCopy.message",
        "library.removeLegacy.message",
        "reader.percentRead.format",
    ] {
        assert!(en.contains_key(key), "English catalog is missing {key}");
        assert!(zh.contains_key(key), "Chinese catalog is missing {key}");
    }
    assert_eq!(zh["library.book.singular"], "本书");
    assert!(zh["library.removeLinked.message"].as_str().is_some_and(|value| {
        value.contains("链接源文件") && value.contains("%1$@")
    }));
}

fn load_strings(path: PathBuf) -> serde_json::Map<String, Value> {
    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&path)
        .output()
        .expect("macOS provides plutil");
    assert!(
        output.status.success(),
        "{} must be a valid strings file: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout)
        .expect("plutil emits JSON")
        .as_object()
        .expect("strings file is a dictionary")
        .clone()
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@main
enum AppleLocalizationSmoke {
    static func main() {
        guard let bundlePath = ProcessInfo.processInfo.environment[
            "READIO_LOCALIZATION_FIXTURE_BUNDLE"
        ], let bundle = Bundle(path: bundlePath) else {
            fatalError("fixture localization bundle is unavailable")
        }
        guard AppleLocalization.normalizedPreference("zh-Hans") == "zh-Hans",
              AppleLocalization.normalizedPreference("fr") == "system",
              AppleLocalization.resolvedLocaleIdentifier(
                  preference: "system",
                  systemLocaleIdentifier: "zh-CN"
              ) == "zh-Hans",
              AppleLocalization.resolvedLocaleIdentifier(
                  preference: "system",
                  systemLocaleIdentifier: "zh_Hans_CN"
              ) == "zh-Hans",
              AppleLocalization.resolvedLocaleIdentifier(
                  preference: "system",
                  systemLocaleIdentifier: "fr-FR"
              ) == "en",
              AppleLocalization.resolvedLocaleIdentifier(
                  preference: "en",
                  systemLocaleIdentifier: "zh-CN"
              ) == "en",
              AppleLocalization.string(
                  "missing.fixture.key",
                  defaultValue: "Fallback",
                  locale: Locale(identifier: "en")
              ) == "Fallback",
              AppleAppDestination.home.localizedTitle(
                  locale: Locale(identifier: "zh-Hans"),
                  bundle: bundle
              ) == "首页",
              AppleLocalization.formatted(
                  "reader.percentRead.format",
                  defaultValue: "%1$lld percent read",
                  locale: Locale(identifier: "zh-Hans"),
                  arguments: [16],
                  bundle: bundle
              ) == "已读 16%" else {
            fatalError("Apple localization policy drifted")
        }
    }
}
"#;
