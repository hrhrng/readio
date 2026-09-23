#![cfg(target_vendor = "apple")]

use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

#[test]
fn readio_application_target_resolves_native_ios_ipados_and_macos_destinations() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let project = repo_root.join("apps/ios/Readio/Readio.xcodeproj/project.pbxproj");

    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&project)
        .output()
        .expect("macOS provides plutil");
    assert!(
        output.status.success(),
        "Xcode project must be a valid property list: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project: Value = serde_json::from_slice(&output.stdout).expect("plutil emits valid JSON");
    let objects = project["objects"]
        .as_object()
        .expect("Xcode project objects dictionary");
    let project_object = objects
        .values()
        .find(|object| object["isa"] == "PBXProject")
        .expect("Xcode project object");
    let project_configuration_list_id = project_object["buildConfigurationList"]
        .as_str()
        .expect("project configuration list reference");
    let project_configuration_ids = objects[project_configuration_list_id]["buildConfigurations"]
        .as_array()
        .expect("project build configurations");
    for configuration_id in project_configuration_ids {
        let configuration = &objects[configuration_id
            .as_str()
            .expect("project build configuration reference")];
        let name = configuration["name"]
            .as_str()
            .expect("project configuration name");
        let settings = configuration["buildSettings"]
            .as_object()
            .expect("project configuration build settings");
        assert_eq!(
            settings["SDKROOT"], "auto",
            "{name} must let the selected native destination choose its Apple SDK"
        );
        assert_eq!(settings["MACOSX_DEPLOYMENT_TARGET"], "14.0");
    }

    let target = objects
        .values()
        .find(|object| {
            object["isa"] == "PBXNativeTarget"
                && object["name"] == "Readio"
                && object["productType"] == "com.apple.product-type.application"
        })
        .expect("Readio application target");
    let configuration_list_id = target["buildConfigurationList"]
        .as_str()
        .expect("application configuration list reference");
    let configuration_ids = objects[configuration_list_id]["buildConfigurations"]
        .as_array()
        .expect("application build configurations");

    assert_eq!(
        configuration_ids.len(),
        2,
        "Debug and Release are both protected"
    );
    for configuration_id in configuration_ids {
        let configuration = &objects[configuration_id
            .as_str()
            .expect("build configuration reference")];
        let name = configuration["name"].as_str().expect("configuration name");
        let settings = configuration["buildSettings"]
            .as_object()
            .expect("configuration build settings");
        let platforms: Vec<_> = settings["SUPPORTED_PLATFORMS"]
            .as_str()
            .expect("explicit supported destinations")
            .split_whitespace()
            .collect();

        for required in ["iphoneos", "iphonesimulator", "macosx"] {
            assert!(
                platforms.contains(&required),
                "{name} must expose the {required} destination; got {platforms:?}"
            );
        }
        assert_eq!(
            settings["MACOSX_DEPLOYMENT_TARGET"], "14.0",
            "{name} must declare the macOS baseline required by the shared SwiftUI surface"
        );
        assert_eq!(
            settings["SUPPORTS_MACCATALYST"], "NO",
            "{name} must publish the native macOS app rather than a Catalyst duplicate"
        );
    }

    let test_target = objects
        .values()
        .find(|object| {
            object["isa"] == "PBXNativeTarget"
                && object["name"] == "ReadioTests"
                && object["productType"] == "com.apple.product-type.bundle.unit-test"
        })
        .expect("Readio unit-test target");
    let test_configuration_list_id = test_target["buildConfigurationList"]
        .as_str()
        .expect("test configuration list reference");
    let test_configuration_ids = objects[test_configuration_list_id]["buildConfigurations"]
        .as_array()
        .expect("test build configurations");

    for configuration_id in test_configuration_ids {
        let configuration = &objects[configuration_id
            .as_str()
            .expect("test build configuration reference")];
        let name = configuration["name"]
            .as_str()
            .expect("test configuration name");
        let settings = configuration["buildSettings"]
            .as_object()
            .expect("test configuration build settings");
        let platforms: Vec<_> = settings["SUPPORTED_PLATFORMS"]
            .as_str()
            .expect("test target destinations")
            .split_whitespace()
            .collect();

        for required in ["iphoneos", "iphonesimulator", "macosx"] {
            assert!(
                platforms.contains(&required),
                "ReadioTests {name} must run on {required}; got {platforms:?}"
            );
        }
        assert_eq!(settings["MACOSX_DEPLOYMENT_TARGET"], "14.0");
    }
}

#[test]
fn readio_application_target_compiles_the_shared_apple_theme_source() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let project = repo_root.join("apps/ios/Readio/Readio.xcodeproj/project.pbxproj");

    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&project)
        .output()
        .expect("macOS provides plutil");
    assert!(output.status.success(), "Xcode project must parse");
    let project: Value = serde_json::from_slice(&output.stdout).expect("valid project JSON");
    let objects = project["objects"].as_object().expect("project objects");

    let theme_file_ids: Vec<_> = objects
        .iter()
        .filter_map(|(id, object)| {
            (object["isa"] == "PBXFileReference"
                && object["path"] == "ReadioAppleTheme.swift")
                .then_some(id.as_str())
        })
        .collect();
    assert_eq!(theme_file_ids.len(), 1, "theme source must have one file reference");

    let target = objects
        .values()
        .find(|object| object["isa"] == "PBXNativeTarget" && object["name"] == "Readio")
        .expect("Readio target");
    let source_phase_ids = target["buildPhases"].as_array().expect("target build phases");
    let source_phase = source_phase_ids
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .find(|object| object["isa"] == "PBXSourcesBuildPhase")
        .expect("Readio sources build phase");
    let source_build_file_ids = source_phase["files"].as_array().expect("source build files");
    let compiles_theme = source_build_file_ids
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .filter_map(|build_file| build_file["fileRef"].as_str())
        .any(|file_id| theme_file_ids.contains(&file_id));
    assert!(compiles_theme, "Readio target must compile ReadioAppleTheme.swift");
}

#[test]
fn readio_application_target_compiles_the_library_forget_presentation_source() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let project = repo_root.join("apps/ios/Readio/Readio.xcodeproj/project.pbxproj");

    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&project)
        .output()
        .expect("macOS provides plutil");
    assert!(output.status.success(), "Xcode project must parse");
    let project: Value = serde_json::from_slice(&output.stdout).expect("valid project JSON");
    let objects = project["objects"].as_object().expect("project objects");

    let presentation_file_ids: Vec<_> = objects
        .iter()
        .filter_map(|(id, object)| {
            (object["isa"] == "PBXFileReference"
                && object["path"] == "LibraryForgetPresentation.swift")
                .then_some(id.as_str())
        })
        .collect();
    assert_eq!(
        presentation_file_ids.len(),
        1,
        "forget presentation source must have one file reference"
    );

    let target = objects
        .values()
        .find(|object| object["isa"] == "PBXNativeTarget" && object["name"] == "Readio")
        .expect("Readio target");
    let source_phase_ids = target["buildPhases"].as_array().expect("target build phases");
    let source_phase = source_phase_ids
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .find(|object| object["isa"] == "PBXSourcesBuildPhase")
        .expect("Readio sources build phase");
    let source_build_file_ids = source_phase["files"].as_array().expect("source build files");
    let compiles_presentation = source_build_file_ids
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .filter_map(|build_file| build_file["fileRef"].as_str())
        .any(|file_id| presentation_file_ids.contains(&file_id));
    assert!(
        compiles_presentation,
        "Readio target must compile LibraryForgetPresentation.swift"
    );
}

#[test]
fn readio_application_target_bundles_english_and_simplified_chinese_catalogs() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let project = repo_root.join("apps/ios/Readio/Readio.xcodeproj/project.pbxproj");

    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&project)
        .output()
        .expect("macOS provides plutil");
    assert!(output.status.success(), "Xcode project must parse");
    let project: Value = serde_json::from_slice(&output.stdout).expect("valid project JSON");
    let objects = project["objects"].as_object().expect("project objects");
    let project_object = objects
        .values()
        .find(|object| object["isa"] == "PBXProject")
        .expect("Xcode project object");
    let regions: Vec<_> = project_object["knownRegions"]
        .as_array()
        .expect("known regions")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(regions.contains(&"en"));
    assert!(
        regions.contains(&"zh-Hans"),
        "Xcode must advertise the Simplified Chinese localization"
    );

    let catalog = objects
        .iter()
        .find(|(_, object)| {
            object["isa"] == "PBXVariantGroup" && object["name"] == "Localizable.strings"
        })
        .expect("localized strings variant group");
    let localized_paths: Vec<_> = catalog
        .1["children"]
        .as_array()
        .expect("localized catalog children")
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|id| objects[id]["path"].as_str())
        .collect();
    assert!(localized_paths.contains(&"en.lproj/Localizable.strings"));
    assert!(localized_paths.contains(&"zh-Hans.lproj/Localizable.strings"));

    let target = objects
        .values()
        .find(|object| object["isa"] == "PBXNativeTarget" && object["name"] == "Readio")
        .expect("Readio target");
    let resources = target["buildPhases"]
        .as_array()
        .expect("target build phases")
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .find(|object| object["isa"] == "PBXResourcesBuildPhase")
        .expect("Readio resources build phase");
    let catalog_id = catalog.0.as_str();
    let bundles_catalog = resources["files"]
        .as_array()
        .expect("resource build files")
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .any(|build_file| build_file["fileRef"].as_str() == Some(catalog_id));
    assert!(
        bundles_catalog,
        "Readio target must copy Localizable.strings into the application bundle"
    );

    let localization_file_ids: Vec<_> = objects
        .iter()
        .filter_map(|(id, object)| {
            (object["isa"] == "PBXFileReference"
                && object["path"] == "AppleLocalization.swift")
                .then_some(id.as_str())
        })
        .collect();
    assert_eq!(
        localization_file_ids.len(),
        1,
        "language policy source must have one file reference"
    );
    let sources = target["buildPhases"]
        .as_array()
        .expect("target build phases")
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .find(|object| object["isa"] == "PBXSourcesBuildPhase")
        .expect("Readio source build phase");
    let compiles_localization = sources["files"]
        .as_array()
        .expect("source build files")
        .iter()
        .filter_map(Value::as_str)
        .map(|id| &objects[id])
        .filter_map(|build_file| build_file["fileRef"].as_str())
        .any(|file_id| localization_file_ids.contains(&file_id));
    assert!(
        compiles_localization,
        "Readio target must compile AppleLocalization.swift"
    );
}
