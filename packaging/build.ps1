# Askway one-click pack script (Inno Setup)
# Usage:
#   .\packaging\build.ps1
#   .\packaging\build.ps1 -SkipBuild
#   .\packaging\build.ps1 -OpenDist
# Or double-click pack.bat at repo root

[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$OpenDist,
    [string]$InnoPath
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$Version = "0.1.0"
$ExeName = "askway.exe"
$ReleaseDir = Join-Path $Root "target\release"
$DistDir = Join-Path $Root "dist"
$IssFile = Join-Path $Root "packaging\askway.iss"
$ExePath = Join-Path $ReleaseDir $ExeName

function Write-Step([string]$Message) {
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Find-Iscc {
    param([string]$Hint)

    if ($Hint) {
        $candidate = if (Test-Path $Hint -PathType Leaf) { $Hint } else { Join-Path $Hint "ISCC.exe" }
        if (Test-Path $candidate) { return (Resolve-Path $candidate).Path }
    }

    $cmd = Get-Command "ISCC.exe" -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $candidates = @(
        "${env:LocalAppData}\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 5\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 5\ISCC.exe"
    )

    foreach ($path in $candidates) {
        if (Test-Path $path) { return $path }
    }

    return $null
}

Write-Host "Askway Packager" -ForegroundColor Green
Write-Host "Root: $Root"

if (-not $SkipBuild) {
    Write-Step "cargo build --release"
    $env:HTTP_PROXY = ""
    $env:HTTPS_PROXY = ""
    $env:http_proxy = ""
    $env:https_proxy = ""
    $env:ALL_PROXY = ""
    $env:all_proxy = ""

    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed, exit code: $LASTEXITCODE"
    }
} else {
    Write-Step "Skip cargo build (-SkipBuild)"
}

if (-not (Test-Path $ExePath)) {
    throw "Executable not found: $ExePath. Run cargo build --release first."
}

$exeInfo = Get-Item $ExePath
$sizeMb = [math]::Round($exeInfo.Length / 1MB, 1)
Write-Host "Exe: $($exeInfo.FullName) (${sizeMb} MB)"

Write-Step "Locate Inno Setup (ISCC.exe)"
$iscc = Find-Iscc -Hint $InnoPath
if (-not $iscc) {
    Write-Host ""
    Write-Host "Inno Setup not found. Install Inno Setup 6:" -ForegroundColor Yellow
    Write-Host "  https://jrsoftware.org/isinfo.php"
    Write-Host ""
    Write-Host "Then retry, or pass the path:"
    Write-Host '  .\packaging\build.ps1 -InnoPath "C:\Program Files (x86)\Inno Setup 6"'
    throw "ISCC.exe not found"
}
Write-Host "ISCC: $iscc"

if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir | Out-Null
}

Write-Step "Compile installer with Inno Setup"
& $iscc `
    "/DSourceDir=$ReleaseDir" `
    "/DOutputDir=$DistDir" `
    "/DMyAppVersion=$Version" `
    $IssFile

if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup compile failed, exit code: $LASTEXITCODE"
}

$installer = Get-ChildItem -Path $DistDir -Filter "Askway_Setup_*.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

Write-Host ""
Write-Host "Done." -ForegroundColor Green
if ($installer) {
    $instMb = [math]::Round($installer.Length / 1MB, 1)
    Write-Host "Installer: $($installer.FullName) (${instMb} MB)"
} else {
    Write-Host "Output dir: $DistDir"
}

if ($OpenDist) {
    Start-Process explorer.exe $DistDir
}
