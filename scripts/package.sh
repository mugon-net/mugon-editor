#!/usr/bin/env bash
# Builds the distributable archive into dist/.
# Usage: scripts/package.sh <version> <platform>
#   platform linux-amd64   -> dist/mugon-editor-<version>-linux-amd64.tar.gz   (from target/release)
#   platform windows-amd64 -> dist/mugon-editor-<version>-windows-amd64.zip    (cross-compiled with cargo-xwin,
#                                                                         from target/x86_64-pc-windows-msvc/release)
set -euo pipefail
version="${1:?version}"
platform="${2:?platform}"
name="mugon-editor-${version}-${platform}"
stage="dist/${name}/mugon-editor"

case "$platform" in
  windows-*) bin_dir="target/x86_64-pc-windows-msvc/release"; ext=".exe" ;;
  *)         bin_dir="target/release";                        ext="" ;;
esac

rm -rf "dist/${name}"
mkdir -p "${stage}/config"
for exe in mugon-editor mugon-editor-thumbs mugon-editor-app; do
  cp "${bin_dir}/${exe}${ext}" "${stage}/"
done
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
