#!/usr/bin/env bash
# Builds the distributable archive into dist/.
# Usage: scripts/package.sh <version> <platform>
#   platform linux-x86_64   -> dist/mugon-editor-<version>-linux-x86_64.tar.gz   (from target/release)
#   platform windows-x86_64 -> dist/mugon-editor-<version>-windows-x86_64.zip    (cross-compiled with cargo-xwin,
#                                                                         from target/x86_64-pc-windows-msvc/release)
set -euo pipefail
version="${1:?version}"
platform="${2:?platform}"
name="mugon-editor-${version}-${platform}"
stage="dist/${name}/mugon-editor"

# FBX2glTF is not part of the repo; it is downloaded (and cached in dist/.cache) when packaging.
fbx_version="v0.9.7"
fbx_base="https://github.com/facebookincubator/FBX2glTF/releases/download/${fbx_version}"
case "$platform" in
  windows-*) bin_dir="target/x86_64-pc-windows-msvc/release"; ext=".exe"; fbx_url="${fbx_base}/FBX2glTF-windows-x64.exe"; fbx_name="FBX2glTF.exe" ;;
  *)         bin_dir="target/release";                        ext="";     fbx_url="${fbx_base}/FBX2glTF-linux-x64";       fbx_name="FBX2glTF" ;;
esac
fbx="dist/.cache/${fbx_version}-${fbx_name}"
if [ ! -f "$fbx" ]; then
  mkdir -p dist/.cache
  curl -fsSL -o "$fbx" "$fbx_url"
fi

rm -rf "dist/${name}"
mkdir -p "${stage}/config"
for exe in mugon-editor mugon-editor-thumbs mugon-editor-app; do
  cp "${bin_dir}/${exe}${ext}" "${stage}/"
done
cp "${fbx}" "${stage}/${fbx_name}"
case "$platform" in windows-*) ;; *) chmod +x "${stage}"/* ;; esac
cp README.md "${stage}/"
cp config/mugon.example.toml "${stage}/config/"

case "$platform" in
  windows-*)
    (cd "dist/${name}" && zip -qr "../${name}.zip" mugon-editor)
    out="dist/${name}.zip" ;;
  *)
    tar -C "dist/${name}" -czf "dist/${name}.tar.gz" mugon-editor
    out="dist/${name}.tar.gz" ;;
esac
rm -rf "dist/${name}"
echo "${out}"
