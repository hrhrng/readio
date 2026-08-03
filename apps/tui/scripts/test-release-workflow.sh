#!/bin/sh
set -eu

workflow="${1:-../../.github/workflows/tui-release.yml}"

require() {
    pattern="$1"
    message="$2"
    grep -Fq "$pattern" "$workflow" || {
        printf 'release workflow: %s\n' "$message" >&2
        exit 1
    }
}

require "pull_request:" "must build release archives on pull requests"
require "aarch64-apple-darwin" "missing Apple Silicon macOS package"
require "x86_64-apple-darwin" "missing Intel macOS package"
require "aarch64-unknown-linux-musl" "missing ARM64 Linux package"
require "x86_64-unknown-linux-musl" "missing x64 Linux package"
require "x86_64-pc-windows-msvc" "missing x64 Windows package"
require "name: verify release archives" "must inspect every packaged archive before publishing"
require "if: github.event_name != 'pull_request'" "must never publish a release from a pull request"

printf 'release workflow contract ok\n'
