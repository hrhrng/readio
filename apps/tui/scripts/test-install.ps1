param(
    [string]$Installer = (Join-Path $PSScriptRoot "install.ps1"),
    [Parameter(Mandatory = $true)]
    [string]$Binary
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Loopback,
        0
    )
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    return $port
}

Assert-True (Test-Path -LiteralPath $Installer -PathType Leaf) "installer is missing: $Installer"
Assert-True (Test-Path -LiteralPath $Binary -PathType Leaf) "test binary is missing: $Binary"

$workspace = Join-Path ([System.IO.Path]::GetTempPath()) ("readio-installer-test-" + [guid]::NewGuid())
$fixtures = Join-Path $workspace "fixtures"
$package = Join-Path $workspace "package"
$installDir = Join-Path $workspace "bin"
$badInstallDir = Join-Path $workspace "bad-bin"
$tag = "tui-v9.9.9-test.1"
$target = "x86_64-pc-windows-msvc"
$archiveName = "readio-$tag-$target.zip"
$archivePath = Join-Path $fixtures $archiveName
$server = $null

try {
    New-Item -ItemType Directory -Force -Path $fixtures, $package | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $package "readio.exe")
    Set-Content -LiteralPath (Join-Path $package "README.md") -Value "fixture"
    Set-Content -LiteralPath (Join-Path $package "LICENSE") -Value "fixture"
    Compress-Archive -Path (Join-Path $package "*") -DestinationPath $archivePath

    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $fixtures "SHA256SUMS") -Value "$hash  $archiveName"
    New-Item -ItemType Directory -Force -Path (Join-Path $fixtures "api") | Out-Null
    @(@{ tag_name = $tag; prerelease = $true }) |
        ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $fixtures "api/releases")

    $port = Get-FreeTcpPort
    $server = Start-Process python -ArgumentList @(
        "-m", "http.server", "$port", "--bind", "127.0.0.1", "--directory", $fixtures
    ) -WindowStyle Hidden -PassThru
    $baseUrl = "http://127.0.0.1:$port"
    $ready = $false
    foreach ($attempt in 1..50) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/releases" | Out-Null
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    Assert-True $ready "fixture HTTP server did not start"

    $env:READIO_API_URL = "$baseUrl/api/releases"
    $env:READIO_BASE_URL = $baseUrl
    $env:READIO_FEED_URL = "$baseUrl/feed"
    $env:READIO_TAG_PREFIX = "tui-v"

    & $Installer -Version latest -InstallDir $installDir
    Assert-True ($LASTEXITCODE -eq 0) "latest install failed"
    $installed = Join-Path $installDir "readio.exe"
    Assert-True (Test-Path -LiteralPath $installed -PathType Leaf) "readio.exe was not installed"
    & $installed --version | Out-Null
    Assert-True ($LASTEXITCODE -eq 0) "installed binary did not run"

    & $Installer -Version $tag -InstallDir $installDir
    Assert-True ($LASTEXITCODE -eq 0) "reinstalling the same version failed"

    Set-Content -LiteralPath (Join-Path $fixtures "SHA256SUMS") -Value (("0" * 64) + "  $archiveName")
    $failed = $false
    try {
        & $Installer -Version $tag -InstallDir $badInstallDir
    } catch {
        $failed = $true
    }
    Assert-True $failed "checksum mismatch should fail"
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $badInstallDir "readio.exe"))) "failed install left an executable behind"
} finally {
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
    Remove-Item Env:READIO_API_URL, Env:READIO_BASE_URL, Env:READIO_FEED_URL, Env:READIO_TAG_PREFIX -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $workspace -Recurse -Force -ErrorAction SilentlyContinue
}
