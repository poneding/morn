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

$buildMsi = -not $Target.StartsWith("aarch64-")
$formats = if ($buildMsi) { '["nsis", "wix"]' } else { '["nsis"]' }

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
formats = $formats
icons = [
  "crates/app/assets/icons/morn-logo-32.png",
  "crates/app/assets/icons/morn-logo-128.png",
  "crates/app/assets/icons/morn-logo-256.png"
]
resources = ["$($runtime.Replace('\', '/'))/*.dll"]

[[binaries]]
path = "morn"
main = true

[[file-associations]]
name = "MPEG-4 Video"
description = "MPEG-4 Video"
ext = ["mp4", "m4v"]

[[file-associations]]
name = "Matroska Video"
description = "Matroska Video"
ext = ["mkv"]

[[file-associations]]
name = "AVI Video"
description = "AVI Video"
ext = ["avi"]

[[file-associations]]
name = "QuickTime Movie"
description = "QuickTime Movie"
ext = ["mov"]

[[file-associations]]
name = "Windows Media Video"
description = "Windows Media Video"
ext = ["wmv"]

[[file-associations]]
name = "Flash Video"
description = "Flash Video"
ext = ["flv"]

[[file-associations]]
name = "WebM Video"
description = "WebM Video"
ext = ["webm"]

[[file-associations]]
name = "MPEG Video"
description = "MPEG Video"
ext = ["mpg", "mpeg"]

[[file-associations]]
name = "MPEG-2 Transport Stream"
description = "MPEG-2 Transport Stream"
ext = ["ts", "mts", "m2ts"]

[[file-associations]]
name = "3GPP Video"
description = "3GPP Video"
ext = ["3gp"]

[[file-associations]]
name = "Ogg Video"
description = "Ogg Video"
ext = ["ogv"]

[[file-associations]]
name = "RealMedia Video"
description = "RealMedia Video"
ext = ["rm", "rmvb"]
"@
$configPath = Join-Path (Get-Location) "Packager-$Suffix.toml"
New-Item -ItemType Directory -Force -Path "dist" | Out-Null
$config | Out-File -Encoding utf8 -FilePath $configPath

cargo packager --release --config $configPath
if ($LASTEXITCODE -ne 0) {
    throw "cargo-packager failed with exit code $LASTEXITCODE"
}

if ($buildMsi) {
    $msi = Get-ChildItem -Path "dist" -Recurse -Filter "*.msi" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $msi) {
        throw "cargo-packager did not produce an MSI"
    }
    Copy-Item $msi.FullName "dist\morn-$releaseTag-$Suffix.msi" -Force
}

$installer = Get-ChildItem -Path "dist" -Recurse -Filter "*.exe" |
    Where-Object { $_.Name -ne "morn.exe" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $installer) {
    throw "cargo-packager did not produce an NSIS installer"
}
Copy-Item $installer.FullName "dist\morn-$releaseTag-$Suffix.exe" -Force
