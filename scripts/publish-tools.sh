#!/usr/bin/env bash
# Adds <version> to the editor index and publishes it with the install scripts to the tools host (a Bunny storage zone).
# Run after the GitHub release exists (reads dist/checksums.txt).
#
# Usage: scripts/publish-tools.sh <version> [requires-cli]     e.g. scripts/publish-tools.sh 0.2.0 ">=0.5.0 <1.0.0"
# Environment:
#   TOOLS_STORAGE_ZONE  storage zone name
#   TOOLS_STORAGE_KEY   zone password (write access)
#   TOOLS_STORAGE_HOST  default storage.bunnycdn.com
#   TOOLS_URL           public tools host, default https://tools.mugon.net (the current index is fetched from there)
#   GITHUB_REPOSITORY   default mugon-net/mugon-editor
#   DRY_RUN=1           write dist/tools/ only, upload nothing
set -euo pipefail

version="${1:?version}"
version="${version#v}"
requires_cli="${2:-}"
repo="${GITHUB_REPOSITORY:-mugon-net/mugon-editor}"
tools_url="${TOOLS_URL:-https://tools.mugon.net}"
checksums="dist/checksums.txt"
out="dist/tools/editor"
mkdir -p "$out"

assets='{}'
while read -r sha file; do
  case "$file" in
    mugon-editor-"${version}"-*.tar.gz) key="${file#mugon-editor-"${version}"-}"; key="${key%.tar.gz}" ;;
    mugon-editor-"${version}"-*.zip) key="${file#mugon-editor-"${version}"-}"; key="${key%.zip}" ;;
    *) continue ;;
  esac
  assets="$(jq -c --arg key "$key" --arg url "https://github.com/${repo}/releases/download/v${version}/${file}" --arg sha "$sha" \
    '. + {($key): {url: $url, sha256: $sha}}' <<< "$assets")"
done < "$checksums"

[ "$assets" != '{}' ] || { echo "no editor archives for ${version} in ${checksums}" >&2; exit 1; }

current="$(curl -fsS "${tools_url}/editor/index.json" 2> /dev/null || echo '{"versions":[]}')"
jq --arg version "$version" --arg requires "$requires_cli" --argjson assets "$assets" \
  '.versions = ((.versions // []) | map(select(.version != $version)) + [{version: $version, requiresCli: $requires, assets: $assets}])' \
  <<< "$current" > "${out}/index.json"
cp scripts/install.sh scripts/install.ps1 "$out/"

if [ "${DRY_RUN:-}" = "1" ]; then
  echo "dry run: wrote ${out}/ (nothing uploaded)"
  exit 0
fi

: "${TOOLS_STORAGE_ZONE:?}" "${TOOLS_STORAGE_KEY:?}"
host="${TOOLS_STORAGE_HOST:-storage.bunnycdn.com}"
for file in install.sh install.ps1 index.json; do
  curl --fail --silent --show-error --request PUT \
    --header "AccessKey: ${TOOLS_STORAGE_KEY}" --header "Content-Type: application/octet-stream" \
    --data-binary "@${out}/${file}" "https://${host}/${TOOLS_STORAGE_ZONE}/editor/${file}"
  echo "uploaded editor/${file}"
done
