#!/usr/bin/env bash
set -euo pipefail

target="${1:?target triple is required}"
suffix="${2:?asset suffix is required}"
deb_arch="${3:?deb architecture is required}"
rpm_arch="${4:?rpm architecture is required}"
appimage_arch="${5:?AppImage architecture is required}"
release_tag="${RELEASE_TAG:?RELEASE_TAG is required}"
linuxdeploy_version="${LINUXDEPLOY_VERSION:-1-alpha-20251107-1}"

version="${release_tag#v}"
deb_version="${version}"
if [[ "${version}" == *-* ]]; then
  deb_version="${version%%-*}~${version#*-}"
fi
rpm_version="${version%%[-+]*}"
rpm_iteration="1"
if [[ "${version}" == *-* ]]; then
  rpm_iteration="${version#*-}"
fi

binary="target/${target}/release/morn"
if [[ ! -x "${binary}" ]]; then
  echo "missing release binary: ${binary}" >&2
  exit 1
fi

pkgroot="dist/pkgroot-${suffix}"
appdir="dist/Morn-${suffix}.AppDir"
icon="crates/app/assets/icons/morn-logo-256.png"
desktop="dist/morn-${suffix}.desktop"
mkdir -p \
  "${pkgroot}/usr/bin" \
  "${pkgroot}/usr/share/applications" \
  "${pkgroot}/usr/share/icons/hicolor/256x256/apps" \
  "${pkgroot}/usr/share/doc/morn" \
  "${appdir}/usr/bin" \
  "${appdir}/usr/share/applications" \
  "${appdir}/usr/share/icons/hicolor/256x256/apps"

cat > "${desktop}" <<'EOF'
[Desktop Entry]
Type=Application
Name=Morn
Comment=Lightweight video player
Exec=morn
Icon=morn
Terminal=false
Categories=AudioVideo;Player;Video;
EOF

install -m 0755 "${binary}" "${pkgroot}/usr/bin/morn"
install -m 0644 "${desktop}" "${pkgroot}/usr/share/applications/morn.desktop"
install -m 0644 "${icon}" "${pkgroot}/usr/share/icons/hicolor/256x256/apps/morn.png"
install -m 0644 README.md "${pkgroot}/usr/share/doc/morn/README.md"

fpm_common=(
  --input-type dir
  --name morn
  --license MIT
  --maintainer "poneding"
  --description "A cross-platform lightweight video player."
  --url "https://github.com/poneding/morn"
  --category "AudioVideo"
  --chdir "${pkgroot}"
)

fpm "${fpm_common[@]}" \
  --output-type deb \
  --version "${deb_version}" \
  --architecture "${deb_arch}" \
  --depends "libgtk-3-0" \
  --depends "libasound2" \
  --depends "libavcodec60" \
  --depends "libavdevice60" \
  --depends "libavfilter9" \
  --depends "libavformat60" \
  --depends "libavutil58" \
  --depends "libswresample4" \
  --depends "libswscale7" \
  --package "dist/morn-${release_tag}-${suffix}.deb" \
  .

fpm "${fpm_common[@]}" \
  --output-type rpm \
  --version "${rpm_version}" \
  --iteration "${rpm_iteration}" \
  --architecture "${rpm_arch}" \
  --package "dist/morn-${release_tag}-${suffix}.rpm" \
  .

cp -a "${pkgroot}/usr" "${appdir}/"
cp "${desktop}" "${appdir}/morn.desktop"
cp "${icon}" "${appdir}/morn.png"
cat > "${appdir}/AppRun" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
here="$(dirname "$(readlink -f "$0")")"
exec "${here}/usr/bin/morn" "$@"
EOF
chmod +x "${appdir}/AppRun"

linuxdeploy_sha256="${LINUXDEPLOY_SHA256:-}"
case "${appimage_arch}" in
  x86_64)
    linuxdeploy_sha256="${LINUXDEPLOY_X86_64_SHA256:-${linuxdeploy_sha256:-c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d}}"
    ;;
  aarch64)
    linuxdeploy_sha256="${LINUXDEPLOY_AARCH64_SHA256:-${linuxdeploy_sha256:-620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff}}"
    ;;
esac

linuxdeploy="dist/linuxdeploy-${appimage_arch}.AppImage"
curl -fsSL \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/${linuxdeploy_version}/linuxdeploy-${appimage_arch}.AppImage" \
  -o "${linuxdeploy}"
if [[ -n "${linuxdeploy_sha256}" ]]; then
  echo "${linuxdeploy_sha256}  ${linuxdeploy}" | sha256sum -c -
fi
chmod +x "${linuxdeploy}"

APPIMAGE_EXTRACT_AND_RUN=1 \
LINUXDEPLOY_OUTPUT_VERSION="${version}" \
LDAI_OUTPUT="$(pwd)/dist/morn-${release_tag}-${suffix}.AppImage" \
"${linuxdeploy}" \
  --appdir "${appdir}" \
  --executable "${appdir}/usr/bin/morn" \
  --desktop-file "${appdir}/usr/share/applications/morn.desktop" \
  --icon-file "${appdir}/usr/share/icons/hicolor/256x256/apps/morn.png" \
  --output appimage

expected_appimage="dist/morn-${release_tag}-${suffix}.AppImage"
if [[ ! -f "${expected_appimage}" ]]; then
  generated_appimage="$(
    find . -maxdepth 2 -type f -name '*.AppImage' \
      ! -path "./${linuxdeploy}" \
      -printf '%T@ %p\n' |
      sort -nr |
      awk 'NR == 1 { print $2 }'
  )"
  if [[ -z "${generated_appimage}" ]]; then
    echo "linuxdeploy did not produce an AppImage" >&2
    exit 1
  fi
  mv "${generated_appimage}" "${expected_appimage}"
fi
