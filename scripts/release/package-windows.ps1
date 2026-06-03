param(
    [Parameter(Mandatory = $true)]
    [string]$Target,
    [Parameter(Mandatory = $true)]
    [string]$Suffix
)

$ErrorActionPreference = "Stop"
$releaseTag = $env:RELEASE_TAG
if (-not $releaseTag) {
    throw "RELEASE_TAG is required"
}
$version = if ($releaseTag.StartsWith("v")) { $releaseTag.Substring(1) } else { $releaseTag }
if ($version -notmatch '^([0-9]+)\.([0-9]+)\.([0-9]+)') {
    throw "RELEASE_TAG must contain a semantic version, got: $releaseTag"
}
$packageVersion = $Matches[0]
$binary = "target\$Target\release\morn.exe"
if (-not (Test-Path $binary)) {
    throw "Missing release binary: $binary"
}

$runtime = "dist\runtime-$Suffix"
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$vcpkgBin = Join-Path $env:VCPKG_ROOT "installed\$env:VCPKG_DEFAULT_TRIPLET\bin"
if (Test-Path $vcpkgBin) {
    Copy-Item "$vcpkgBin\*.dll" $runtime -ErrorAction SilentlyContinue
}

$config = @"
name = "morn"
product-name = "Morn"
identifier = "com.poneding.morn"
version = "$packageVersion"
publisher = "poneding"
description = "A cross-platform lightweight video player."
category = "Video"
homepage = "https://github.com/poneding/morn"
out-dir = "dist"
binaries-dir = "target/$Target/release"
target-triple = "$Target"
formats = ["nsis", "wix"]
icons = [
  "crates/app/assets/icons/morn-logo-32.png",
  "crates/app/assets/icons/morn-logo-128.png",
  "crates/app/assets/icons/morn-logo-256.png"
]
resources = ["$($runtime.Replace('\', '/'))/*.dll"]

[[binaries]]
path = "morn"
main = true
"@
$configPath = "dist\Packager-$Suffix.toml"
New-Item -ItemType Directory -Force -Path "dist" | Out-Null
$config | Out-File -Encoding utf8 -FilePath $configPath

cargo packager --release --config $configPath

$msi = Get-ChildItem -Path "dist" -Recurse -Filter "*.msi" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $msi) {
    throw "cargo-packager did not produce an MSI"
}
Copy-Item $msi.FullName "dist\morn-$releaseTag-$Suffix.msi" -Force

$installer = Get-ChildItem -Path "dist" -Recurse -Filter "*.exe" |
    Where-Object { $_.Name -ne "morn.exe" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $installer) {
    throw "cargo-packager did not produce an NSIS installer"
}
Copy-Item $installer.FullName "dist\morn-$releaseTag-$Suffix.exe" -Force
