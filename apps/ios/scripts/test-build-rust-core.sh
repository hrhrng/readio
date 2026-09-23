#!/bin/bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build_script="$script_dir/build-rust-core.sh"

assert_plan() {
    local platform_name="$1"
    local effective_platform_name="$2"
    local architectures="$3"
    local expected_targets="$4"

    local actual
    actual="$({
        READIO_APPLE_BUILD_DRY_RUN=1 \
        PLATFORM_NAME="$platform_name" \
        EFFECTIVE_PLATFORM_NAME="$effective_platform_name" \
        ARCHS="$architectures" \
        CONFIGURATION=Debug \
        SRCROOT="$script_dir/../Readio" \
        DERIVED_FILE_DIR=/tmp/readio-derived \
        /bin/bash "$build_script"
    })"

    [[ "$actual" == *"rust-targets=$expected_targets"* ]]
    [[ "$actual" == *"rust-profile=debug"* ]]
    [[ "$actual" == *"rust-library=/tmp/readio-derived/readio-core/libreadio_ffi.a"* ]]
}

assert_plan iphoneos "-iphoneos" "arm64" "aarch64-apple-ios"
assert_plan iphonesimulator "-iphonesimulator" "arm64 x86_64" \
    "aarch64-apple-ios-sim x86_64-apple-ios"
assert_plan macosx "-maccatalyst" "arm64 x86_64" \
    "aarch64-apple-ios-macabi x86_64-apple-ios-macabi"
assert_plan macosx "" "arm64" "aarch64-apple-darwin"

echo "Apple Rust build mapping tests passed"
