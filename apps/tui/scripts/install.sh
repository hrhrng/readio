#!/bin/sh
# readio installer.
#
#   curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
#
# Downloads the release binary for this machine, checks it against the release's
# SHA-256 list, and puts it in ~/.local/bin. No sudo, no compiler, no Rust
# toolchain, nothing written outside the install directory.
#
# The two knobs are arguments, not magic:
#   install.sh --version tui-v0.2.0-beta.2   a release other than the latest
#   install.sh --dir /usr/local/bin          somewhere else (bring write access)
#
# POSIX sh on purpose: this has to run under dash, busybox ash and macOS's old
# bash without anyone thinking about it.

set -eu

REPO="${READIO_REPO:-hrhrng/readio}"
# Where releases live. Overridable so this script can be tested against a local
# server instead of only in production, which is the difference between an
# installer that is believed to work and one that is known to.
BASE_URL="${READIO_BASE_URL:-https://github.com/$REPO/releases/download}"
API_URL="${READIO_API_URL:-https://api.github.com/repos/$REPO/releases}"
# readio lives in a monorepo whose other apps release under their own tags, so
# "latest" means the newest tag with this prefix, not whatever shipped last.
TAG_PREFIX="${READIO_TAG_PREFIX:-tui-v}"
VERSION="latest"
BIN_DIR="${HOME}/.local/bin"
TMP_DIR=""

log() { printf '%s\n' "$*" >&2; }
die() {
    printf 'readio: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    # An `if` rather than a `&&` chain: this runs from the EXIT trap, and in
    # bash and dash the trap's last command decides the script's exit status —
    # a false `[ -n "$TMP_DIR" ]` would turn `--help` into a failure.
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT INT TERM

usage() {
    cat <<'EOF'
Usage: install.sh [--version <tag>] [--dir <path>]

  --version <tag>   release to install, e.g. tui-v0.2.0-beta.2 (default: latest)
  --dir <path>      install directory (default: ~/.local/bin)
  --help            this message
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) [ $# -ge 2 ] || die "--version needs a tag"; VERSION="$2"; shift 2 ;;
        --version=*) VERSION="${1#--version=}"; shift ;;
        --dir) [ $# -ge 2 ] || die "--dir needs a path"; BIN_DIR="$2"; shift 2 ;;
        --dir=*) BIN_DIR="${1#--dir=}"; shift ;;
        --help | -h) usage; exit 0 ;;
        *) die "unknown argument $1 (try --help)" ;;
    esac
done

# ── what are we running on ───────────────────────────────────────────────────

# POSIX sh has no local variables, so everything inside a function is prefixed
# with `_`: an earlier version of this script had verify() clobber the global
# $archive, and tar was handed a doubled path.
detect_target() {
    _os="$(uname -s)"
    _arch="$(uname -m)"
    case "$_os" in
        Darwin)
            case "$_arch" in
                arm64 | aarch64) echo "aarch64-apple-darwin" ;;
                x86_64) echo "x86_64-apple-darwin" ;;
                *) die "unsupported macOS architecture $_arch" ;;
            esac
            ;;
        Linux)
            # musl builds are static, so one Linux artifact per architecture
            # works on any distribution, glibc version notwithstanding.
            case "$_arch" in
                aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
                x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
                *) die "unsupported Linux architecture $_arch" ;;
            esac
            ;;
        MINGW* | MSYS* | CYGWIN*)
            die "Windows is not packaged yet; build it from apps/tui with: cargo build --release"
            ;;
        *) die "unsupported system $_os" ;;
    esac
}

# ── downloading ──────────────────────────────────────────────────────────────

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
    fetch_stdout() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
    fetch_stdout() { wget -qO- "$1"; }
else
    die "need curl or wget"
fi

resolve_version() {
    [ "$VERSION" != "latest" ] && { echo "$VERSION"; return; }
    # Ask the API for the latest tag. Parsed with sed rather than jq, which is
    # not installed often enough to depend on. The list endpoint is used rather
    # than /releases/latest on purpose: readio is in beta, its releases are
    # flagged as prereleases, and /releases/latest skips those entirely.
    # `tr` first: the API is pretty-printed today, but a compact body would put
    # every tag on one line and the greedy `.*` would then return the oldest
    # release instead of the newest. Splitting on commas keeps the API's
    # newest-first order while giving sed one field per line.
    _tag="$(fetch_stdout "$API_URL" \
        | tr ',' '\n' \
        | sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p' \
        | grep "^$TAG_PREFIX" | head -n 1)"
    [ -n "$_tag" ] || die "no $TAG_PREFIX* release found in $REPO
Releases: https://github.com/$REPO/releases"
    echo "$_tag"
}

# ── verification ─────────────────────────────────────────────────────────────

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | awk '{print $NF}'
    else
        echo ""
    fi
}

verify() {
    _file="$1"
    _sums="$2"
    _name="$3"
    _actual="$(sha256_of "$_file")"
    if [ -z "$_actual" ]; then
        log "readio: no sha256 tool found, skipping the checksum check"
        return 0
    fi
    # sha256sum writes "<hash>  <name>", shasum "<hash>  <name>" too, and some
    # tools mark binary mode with an asterisk. Match the name at end of line.
    _expected="$(grep -E "[[:space:]]\*?$_name\$" "$_sums" 2>/dev/null \
        | head -n 1 | cut -d' ' -f1)"
    [ -n "$_expected" ] || die "$_name is not listed in the release checksums"
    [ "$_actual" = "$_expected" ] || die "checksum mismatch for $_name
  expected $_expected
  got      $_actual
Refusing to install. Download it yourself from
  https://github.com/$REPO/releases"
}

# ── install ──────────────────────────────────────────────────────────────────

target="$(detect_target)"
version="$(resolve_version)"
archive="readio-${version}-${target}.tar.gz"
base="$BASE_URL/$version"

TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t readio)"
log "readio: downloading $version for $target"
fetch "$base/$archive" "$TMP_DIR/$archive" \
    || die "no build for $target in $version — see https://github.com/$REPO/releases"

if fetch "$base/SHA256SUMS" "$TMP_DIR/SHA256SUMS" 2>/dev/null; then
    verify "$TMP_DIR/$archive" "$TMP_DIR/SHA256SUMS" "$archive"
else
    log "readio: this release publishes no checksums, installing unverified"
fi

# -C first: BSD and GNU tar both honour it that way round.
tar -C "$TMP_DIR" -xzf "$TMP_DIR/$archive" || die "cannot unpack $archive"
[ -f "$TMP_DIR/readio" ] || die "$archive does not contain a readio binary"

mkdir -p "$BIN_DIR" || die "cannot create $BIN_DIR"
# Install by rename so an upgrade cannot leave a half-written binary behind, and
# so replacing a running readio works.
mv "$TMP_DIR/readio" "$BIN_DIR/readio.new" || die "cannot write to $BIN_DIR"
chmod 755 "$BIN_DIR/readio.new"
mv "$BIN_DIR/readio.new" "$BIN_DIR/readio" || die "cannot install into $BIN_DIR"

installed="$("$BIN_DIR/readio" --version 2>/dev/null || echo "readio")"
log "readio: installed $installed → $BIN_DIR/readio"

case ":$PATH:" in
    *":$BIN_DIR:"*) log "readio: run 'readio' to start" ;;
    *)
        log ""
        log "readio: $BIN_DIR is not on your PATH. Add it:"
        log "  echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc   # or ~/.bashrc"
        log "Or run it directly: $BIN_DIR/readio"
        ;;
esac
