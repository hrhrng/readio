---
name: setup-readio
description: Install, update, verify, configure, troubleshoot, or uninstall hrhrng/readio, including Windows, macOS, Linux, PATH setup, interface language, reading modes, local voice models, per-book voices, and audio-output safety. Use this skill whenever a user mentions downloading, installing, setting up, updating, running, configuring, or removing readio, even if they do not explicitly ask for a skill.
compatibility: Official prebuilt installers for macOS/Linux aarch64 or x86_64 and Windows x64; source build elsewhere. Requires a terminal and network access.
---

# Set up readio

Install and configure the terminal ebook reader from its official sources:

- Repository: <https://github.com/hrhrng/readio>
- Releases: <https://github.com/hrhrng/readio/releases>
- macOS/Linux installer: <https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh>
- Windows installer: <https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.ps1>

Inspect first, change only what the user requested, verify the result, and end
with the exact binary/config paths and one command the user can run next.

## Safety

- Use only `hrhrng/readio` and its release assets.
- Normal installation needs no `sudo`, compiler, or Rust toolchain.
- Download an installer to a visible temporary path before running it. State
  the source and destination; do not hide a remote script inside a pipe.
- Both installers require the archive to match the release `SHA256SUMS`.
  Missing or mismatched checksums are hard failures; never bypass them.
- Installing or updating the binary must not alter the library, progress, or
  configuration under `~/.readio` or `%USERPROFILE%\.readio`.
- Before editing `config.yaml`, make a timestamped sibling backup. Preserve
  comments, unknown keys, custom engines, and per-book entries.
- Voice downloads may be large and may install a managed Python runtime. Let
  readio show size, licence, and free-space checks, then obtain confirmation.
- Never remove user data or model caches without explicit confirmation.

## 1. Read-only preflight

Identify the OS, architecture, existing executable, and data directory. On
macOS/Linux:

```sh
uname -s
uname -m
command -v readio || true
readio --version 2>/dev/null || true
printf '%s\n' "$PATH"
test -f "$HOME/.readio/config.yaml" && echo "config exists"
test -d "$HOME/.readio/books" && echo "library exists"
```

On Windows PowerShell:

```powershell
[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
Get-Command readio -ErrorAction SilentlyContinue
readio --version
Test-Path "$HOME\.readio\config.yaml"
Test-Path "$HOME\.readio\books"
```

If readio exists, determine whether the user wants an update, configuration
help, or only a health check. Re-running the installer is the update path.

## 2. Download and install

### macOS or Linux (aarch64/x86_64)

Use the official prebuilt installer:

```sh
readio_installer_dir="$(mktemp -d)"
curl -fsSL \
  https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh \
  -o "$readio_installer_dir/install.sh"
sh "$readio_installer_dir/install.sh" --help
sh "$readio_installer_dir/install.sh"
```

It selects the newest `tui-v*` release, verifies it, and installs
`~/.local/bin/readio`. For an explicit, already verified release or writable
destination:

```sh
sh "$readio_installer_dir/install.sh" --version tui-vX.Y.Z
sh "$readio_installer_dir/install.sh" --dir /explicit/writable/bin
```

Resolve real tags from Releases; never invent a version.

### Windows

Use the official x64 PowerShell installer:

```powershell
$installer = Join-Path $env:TEMP "readio-install.ps1"
Invoke-WebRequest https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.ps1 -OutFile $installer
Get-Help $installer
& $installer
```

It downloads the x64 ZIP, requires a matching `SHA256SUMS`, and installs
`%USERPROFILE%\.local\bin\readio.exe` without touching
`%USERPROFILE%\.readio`. Windows ARM64 uses the x64 artifact through Windows
emulation. Verify the installed file directly:

```powershell
& "$HOME\.local\bin\readio.exe" --version
& "$HOME\.local\bin\readio.exe" --help
```

Windows x64 is an official prebuilt route. Pull-request CI builds and launches
the release executable on a native Windows runner, packages and inspects the
ZIP, and exercises first install, reinstall, checksum verification, and
checksum rejection. Do not describe Windows as source-only or untested.

### Other systems

If Rust 1.90+ already exists, offer the source route:

```sh
cargo install --git https://github.com/hrhrng/readio readio
```

Do not install Rust, Visual Studio Build Tools, or MinGW without explaining
size and impact and obtaining permission.

## 3. Verify discovery and PATH

Always verify the absolute executable first, then PATH discovery:

```sh
"$HOME/.local/bin/readio" --version
"$HOME/.local/bin/readio" --help
command -v readio
```

On Windows use the direct commands above, then `Get-Command readio`. If the
install directory is absent from PATH, propose the smallest correct change for
the user's shell or Windows user PATH. Do not edit profiles or environment
variables without authorization; offer the absolute executable meanwhile.

## 4. Configuration routing

When the request includes language, modes, pace, images, voice models,
audiobook language/voice, per-book overrides, output-device safety, or direct
YAML editing, read [references/configuration.md](references/configuration.md)
before changing anything. That reference contains the canonical settings,
backup procedure, model choices, and verification sequence.

The default config is `~/.readio/config.yaml` (also under `%USERPROFILE%` on
Windows). `readio --home <dir>` relocates the entire host directory, so inspect
the user's launch command before assuming the default. If no config exists,
start readio in a real terminal and exit normally so the current release writes
its annotated defaults; do not fabricate the full file.

## 5. Basic handoff

```text
readio                 open the library
readio book.epub       copy a book into the library and read
readio book.pdf -l     link the original file
readio book.md -m      move the file into the library
```

Inside readio: `/sample`, `/import <path>`, `/toc`, `/find <term>`, `/voice`,
and `/help` cover the common next steps.

## 6. Uninstall only when requested

Offer binary-only removal first:

- Unix binary: `~/.local/bin/readio`
- Windows binary: `%USERPROFILE%\.local\bin\readio.exe`
- User data: `~/.readio` or `%USERPROFILE%\.readio`
- Managed voice runtimes: platform user cache under `readio`

Before removing data or caches, state exactly what will be lost and whether a
backup exists, then wait for explicit confirmation.

## Completion report

Return a compact report:

```text
readio: <installed version or not installed>
binary: <absolute path>
config: <absolute path>
backup: <path or none>
changed: <exact settings changed>
verified: <version/help/config/TUI/voice checks performed>
next: <one command to run now>
```

State anything unverified instead of presenting partial setup as complete.
