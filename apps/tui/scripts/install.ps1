<#
.SYNOPSIS
Installs a verified readio Windows release for the current user.

.PARAMETER Version
Release tag to install, or "latest" (the default).

.PARAMETER InstallDir
Destination directory. Defaults to %USERPROFILE%\.local\bin.
#>
param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $HOME ".local\bin")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repo = if ($env:READIO_REPO) { $env:READIO_REPO } else { "hrhrng/readio" }
$baseUrl = if ($env:READIO_BASE_URL) { $env:READIO_BASE_URL.TrimEnd("/") } else { "https://github.com/$repo/releases/download" }
$apiUrl = if ($env:READIO_API_URL) { $env:READIO_API_URL } else { "https://api.github.com/repos/$repo/releases" }
$feedUrl = if ($env:READIO_FEED_URL) { $env:READIO_FEED_URL } else { "https://github.com/$repo/releases.atom" }
$tagPrefix = if ($env:READIO_TAG_PREFIX) { $env:READIO_TAG_PREFIX } else { "tui-v" }
$tempDir = $null

function Write-Log([string]$Message) {
    [Console]::Error.WriteLine($Message)
}

function Fail([string]$Message) {
    throw "readio: $Message"
}

function Download-File([string]$Uri, [string]$Destination, [int]$Attempts = 1) {
    foreach ($attempt in 1..$Attempts) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
            return
        } catch {
            if ($attempt -eq $Attempts) { throw }
            Start-Sleep -Seconds 1
        }
    }
}

function Resolve-Version {
    if ($Version -ne "latest") { return $Version }

    try {
        $releases = Invoke-RestMethod -Uri $apiUrl
        $match = $releases |
            Where-Object { $_.tag_name -and $_.tag_name.StartsWith($tagPrefix) } |
            Select-Object -First 1
        if ($match) { return [string]$match.tag_name }
    } catch {
        Write-Log "readio: GitHub API lookup failed; trying the releases feed"
    }

    try {
        $feed = (Invoke-WebRequest -UseBasicParsing -Uri $feedUrl).Content
        $escapedPrefix = [regex]::Escape($tagPrefix)
        $match = [regex]::Match($feed, "/releases/tag/($escapedPrefix[^`"'<]+)")
        if ($match.Success) { return $match.Groups[1].Value }
    } catch {
        Write-Log "readio: releases feed lookup failed"
    }

    Fail "cannot determine the latest $tagPrefix release. Pass -Version $tagPrefix<x.y.z>."
}

try {
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) {
        Fail "install.ps1 is for Windows; use scripts/install.sh on macOS or Linux"
    }

    $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($architecture) {
        "X64" { $target = "x86_64-pc-windows-msvc" }
        "Arm64" {
            $target = "x86_64-pc-windows-msvc"
            Write-Log "readio: using the x64 build through Windows ARM emulation"
        }
        default { Fail "unsupported Windows architecture $architecture" }
    }

    $resolvedVersion = Resolve-Version
    $archiveName = "readio-$resolvedVersion-$target.zip"
    $releaseUrl = "$baseUrl/$resolvedVersion"
    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("readio-install-" + [guid]::NewGuid())
    $archivePath = Join-Path $tempDir $archiveName
    $checksumPath = Join-Path $tempDir "SHA256SUMS"
    $unpackDir = Join-Path $tempDir "unpacked"
    New-Item -ItemType Directory -Force -Path $tempDir, $unpackDir | Out-Null

    Write-Log "readio: downloading $resolvedVersion for $target"
    try {
        Download-File "$releaseUrl/$archiveName" $archivePath
    } catch {
        Fail "no build for $target in $resolvedVersion ($releaseUrl/$archiveName)"
    }
    try {
        Download-File "$releaseUrl/SHA256SUMS" $checksumPath 3
    } catch {
        Fail "could not fetch SHA256SUMS for $resolvedVersion after three tries"
    }

    $escapedName = [regex]::Escape($archiveName)
    $checksumLine = Get-Content -LiteralPath $checksumPath |
        Where-Object { $_ -match "^([0-9a-fA-F]{64})\s+\*?$escapedName$" } |
        Select-Object -First 1
    if (-not $checksumLine) { Fail "$archiveName is not listed in SHA256SUMS" }
    $expected = ([regex]::Match($checksumLine, "^[0-9a-fA-F]{64}")).Value.ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        Fail "checksum mismatch for $archiveName`n  expected $expected`n  got      $actual`nRefusing to install."
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $unpackDir
    $source = Join-Path $unpackDir "readio.exe"
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        Fail "$archiveName does not contain readio.exe at its root"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $destination = Join-Path $InstallDir "readio.exe"
    $staged = Join-Path $InstallDir "readio.exe.new"
    Copy-Item -LiteralPath $source -Destination $staged -Force
    if (Test-Path -LiteralPath $destination -PathType Leaf) {
        [System.IO.File]::Replace($staged, $destination, $null)
    } else {
        Move-Item -LiteralPath $staged -Destination $destination
    }

    $installedVersion = (& $destination --version | Select-Object -First 1)
    if (-not $installedVersion -or -not $installedVersion.StartsWith("readio ")) {
        Fail "the installed readio.exe did not return a valid version"
    }
    Write-Log "readio: installed $installedVersion -> $destination"

    $pathEntries = $env:PATH -split ";" | ForEach-Object { $_.TrimEnd("\") }
    if ($pathEntries -contains $InstallDir.TrimEnd("\")) {
        Write-Log "readio: run 'readio' to start"
    } else {
        Write-Log ""
        Write-Log "readio: $InstallDir is not on PATH. Run it directly:"
        Write-Log "  & '$destination'"
        Write-Log "Or add that directory to your user PATH in Windows Settings."
    }
} finally {
    if ($tempDir -and (Test-Path -LiteralPath $tempDir)) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
