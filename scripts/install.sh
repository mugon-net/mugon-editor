#!/bin/sh
# Installs the Mugon Editor through the mugon CLI (installing the CLI first if needed).
# Served at https://tools.mugon.net/editor/install.sh.
#   MUGON_TOOLS_URL  tools host (default https://tools.mugon.net)
set -e

tools_url="${MUGON_TOOLS_URL:-https://tools.mugon.net}"

if ! command -v mugon >/dev/null 2>&1 && [ ! -x "$HOME/.mugon/bin/mugon" ]; then
  echo "Installing the mugon CLI first..."
  curl -fsSL "${tools_url}/cli/install.sh" | sh
fi

if command -v mugon >/dev/null 2>&1; then
  mugon_bin=mugon
else
  mugon_bin="$HOME/.mugon/bin/mugon"
fi

"$mugon_bin" editor install

echo "Next: run 'mugon editor --help'. The scene editor and thumbnail renderer need a Vulkan-capable GPU driver."
