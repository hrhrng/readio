#!/bin/bash

set -euo pipefail

: "${PLATFORM_NAME:?Xcode must provide PLATFORM_NAME}"
: "${ARCHS:?Xcode must provide ARCHS}"
: "${CONFIGURATION:?Xcode must provide CONFIGURATION}"
: "${SRCROOT:?Xcode must provide SRCROOT}"
: "${DERIVED_FILE_DIR:?Xcode must provide DERIVED_FILE_DIR}"

effective_platform_name="${EFFECTIVE_PLATFORM_NAME:-}"
rust_targets=()

for architecture in $ARCHS; do
    case "$PLATFORM_NAME:$effective_platform_name:$architecture" in
        iphoneos:*:arm64)
            rust_targets+=("aarch64-apple-ios")
            ;;
        iphonesimulator:*:arm64)
            rust_targets+=("aarch64-apple-ios-sim")
            ;;
        iphonesimulator:*:x86_64)
            rust_targets+=("x86_64-apple-ios")
            ;;
        macosx:-maccatalyst:arm64)
            rust_targets+=("aarch64-apple-ios-macabi")
            ;;
        macosx:-maccatalyst:x86_64)
            rust_targets+=("x86_64-apple-ios-macabi")
            ;;
        macosx:*:arm64)
            rust_targets+=("aarch64-apple-darwin")
            ;;
        macosx:*:x86_64)
            rust_targets+=("x86_64-apple-darwin")
            ;;
        *)
            echo "Unsupported Apple build tuple: $PLATFORM_NAME/$effective_platform_name/$architecture" >&2
            exit 1
            ;;
    esac
done

profile=debug
if [[ "$CONFIGURATION" == "Release" ]]; then
    profile=release
fi

output_directory="$DERIVED_FILE_DIR/readio-core"
output_library="$output_directory/libreadio_ffi.a"

if [[ "${READIO_APPLE_BUILD_DRY_RUN:-0}" == "1" ]]; then
    echo "rust-targets=${rust_targets[*]}"
    echo "rust-profile=$profile"
    echo "rust-library=$output_library"
    exit 0
fi

workspace_root="$(cd "$SRCROOT/../../.." && pwd)"
rustup_bin="$(command -v rustup || true)"
cargo_bin="$(command -v cargo || true)"

if [[ -z "$rustup_bin" || -z "$cargo_bin" ]]; then
    echo "Rustup and Cargo must be available in Xcode's PATH" >&2
    exit 1
fi

installed_targets="$($rustup_bin target list --installed --toolchain stable)"
input_libraries=()

mkdir -p "$output_directory"

for rust_target in "${rust_targets[@]}"; do
    if ! grep -qx "$rust_target" <<<"$installed_targets"; then
        echo "Missing Rust target $rust_target; run: rustup target add $rust_target --toolchain stable" >&2
        exit 1
    fi

    cargo_arguments=(
        build
        --manifest-path "$workspace_root/Cargo.toml"
        --package readio-ffi
        --target "$rust_target"
    )
    if [[ "$profile" == "release" ]]; then
        cargo_arguments+=(--release)
    fi

    RUSTC="$($rustup_bin which rustc --toolchain stable)" \
    RUSTDOC="$($rustup_bin which rustdoc --toolchain stable)" \
        "$cargo_bin" "${cargo_arguments[@]}"
    input_libraries+=("$workspace_root/target/$rust_target/$profile/libreadio_ffi.a")
done

if [[ "${#input_libraries[@]}" == "1" ]]; then
    cp "${input_libraries[0]}" "$output_library"
else
    xcrun lipo -create "${input_libraries[@]}" -output "$output_library"
fi
