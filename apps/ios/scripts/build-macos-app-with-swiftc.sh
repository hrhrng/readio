#!/bin/bash

set -euo pipefail

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../../.." && pwd)"
apple_source="$script_directory/../Readio/Readio"
output_bundle="$script_directory/../dist/Readio.app"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output)
            [[ $# -ge 2 ]] || {
                echo "--output requires an app-bundle path" >&2
                exit 2
            }
            output_bundle="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if [[ "$output_bundle" != *.app ]]; then
    echo "Output must end in .app: $output_bundle" >&2
    exit 2
fi

ffi_library="${READIO_FFI_LIBRARY:-}"
if [[ -z "$ffi_library" ]]; then
    cargo build \
        --manifest-path "$repository_root/Cargo.toml" \
        --package readio-ffi
    ffi_library="$repository_root/target/debug/libreadio_ffi.a"
    if [[ ! -f "$ffi_library" ]]; then
        ffi_library="$(find "$repository_root/target/debug/deps" \
            -maxdepth 1 -type f -name 'libreadio_ffi.a' -print -quit)"
    fi
fi
if [[ ! -f "$ffi_library" ]]; then
    echo "Rust static library not found: $ffi_library" >&2
    exit 1
fi

temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/readio-macos-build.XXXXXX")"
trap 'rm -rf "$temporary_directory"' EXIT
sanitized_sources="$temporary_directory/sources"
staged_bundle="$temporary_directory/Readio.app"
mkdir -p \
    "$sanitized_sources" \
    "$staged_bundle/Contents/MacOS" \
    "$staged_bundle/Contents/Resources"

source_index=0
swift_sources=()
while IFS= read -r source; do
    destination="$sanitized_sources/$(printf '%03d' "$source_index")-$(basename "$source")"
    # Command Line Tools include SwiftUI but not Xcode's design-time preview
    # macro plugin. Product declarations precede #Preview in every source file,
    # so the CLI packager compiles exactly that production portion.
    awk '/^#Preview([ (]|$)/ { preview = 1 } !preview { print }' \
        "$source" > "$destination"
    swift_sources+=("$destination")
    source_index=$((source_index + 1))
done < <(find "$apple_source" -type f -name '*.swift' -print | sort)

swiftc \
    -parse-as-library \
    -module-name Readio \
    -g \
    -I "$repository_root/crates/readio-ffi/include" \
    "${swift_sources[@]}" \
    "$ffi_library" \
    -framework AppKit \
    -framework AVFoundation \
    -framework ImageIO \
    -framework MediaPlayer \
    -framework SwiftUI \
    -o "$staged_bundle/Contents/MacOS/Readio"

cp "$apple_source/Resources/Info.plist" "$staged_bundle/Contents/Info.plist"
for localization_directory in "$apple_source"/Resources/*.lproj; do
    [[ -d "$localization_directory" ]] || continue
    cp -R "$localization_directory" "$staged_bundle/Contents/Resources/"
done
plist="$staged_bundle/Contents/Info.plist"
set_plist_string() {
    local key="$1"
    local value="$2"
    /usr/libexec/PlistBuddy -c "Set :$key $value" "$plist" 2>/dev/null || \
        /usr/libexec/PlistBuddy -c "Add :$key string $value" "$plist"
}
set_plist_bool() {
    local key="$1"
    local value="$2"
    /usr/libexec/PlistBuddy -c "Set :$key $value" "$plist" 2>/dev/null || \
        /usr/libexec/PlistBuddy -c "Add :$key bool $value" "$plist"
}

set_plist_string CFBundleDevelopmentRegion en
set_plist_string CFBundleExecutable Readio
set_plist_string CFBundleIdentifier com.readio.app
set_plist_string CFBundleInfoDictionaryVersion 6.0
set_plist_string CFBundleName Readio
set_plist_string CFBundlePackageType APPL
set_plist_string CFBundleShortVersionString 0.2.0-beta.12
set_plist_string CFBundleVersion 1
set_plist_string LSMinimumSystemVersion 14.0
set_plist_bool NSHighResolutionCapable true

codesign --force --deep --sign - "$staged_bundle"

output_parent="$(dirname "$output_bundle")"
mkdir -p "$output_parent"
if [[ -e "$output_bundle" ]]; then
    rm -rf "$output_bundle"
fi
mv "$staged_bundle" "$output_bundle"

echo "$output_bundle"
