#!/usr/bin/env bash
set -euo pipefail

target="${1:?target triple is required}"
suffix="${2:?asset suffix is required}"
release_tag="${RELEASE_TAG:?RELEASE_TAG is required}"
version="${release_tag#v}"
package_version="${version%%[-+]*}"

binary="target/${target}/release/morn"
if [[ ! -x "${binary}" ]]; then
  echo "missing release binary: ${binary}" >&2
  exit 1
fi

app="dist/Morn.app"
contents="${app}/Contents"
macos="${contents}/MacOS"
resources="${contents}/Resources"
frameworks="${contents}/Frameworks"
iconset="dist/Morn.iconset"
icns="${resources}/Morn.icns"

rm -rf "${app}" "${iconset}"
mkdir -p "${macos}" "${resources}" "${frameworks}" "${iconset}"

cp "${binary}" "${macos}/Morn"
chmod +x "${macos}/Morn"

sips -z 16 16 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_16x16.png" >/dev/null
sips -z 32 32 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_16x16@2x.png" >/dev/null
sips -z 32 32 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_32x32.png" >/dev/null
sips -z 64 64 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_32x32@2x.png" >/dev/null
sips -z 128 128 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_128x128.png" >/dev/null
sips -z 256 256 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_128x128@2x.png" >/dev/null
sips -z 256 256 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_256x256.png" >/dev/null
sips -z 512 512 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_256x256@2x.png" >/dev/null
sips -z 512 512 crates/app/assets/icons/morn-logo-1024.png --out "${iconset}/icon_512x512.png" >/dev/null
cp crates/app/assets/icons/morn-logo-1024.png "${iconset}/icon_512x512@2x.png"
iconutil -c icns "${iconset}" -o "${icns}"

cat > "${contents}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Morn</string>
  <key>CFBundleExecutable</key>
  <string>Morn</string>
  <key>CFBundleIconFile</key>
  <string>Morn</string>
  <key>CFBundleIdentifier</key>
  <string>com.poneding.morn</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Morn</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${package_version}</string>
  <key>CFBundleVersion</key>
  <string>${package_version}</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.video</string>
</dict>
</plist>
EOF

dylibbundler \
  -od \
  -b \
  -x "${macos}/Morn" \
  -d "${frameworks}" \
  -p "@executable_path/../Frameworks/"

codesign --force --deep --sign - "${app}"
hdiutil create \
  -volname "Morn" \
  -srcfolder "${app}" \
  -ov \
  -format UDZO \
  "dist/morn-${release_tag}-${suffix}.dmg"
