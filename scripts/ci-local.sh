#!/usr/bin/env bash
# Reproduces the CI build jobs locally in a resource-limited docker container (default: 4 GB RAM, 2 CPUs),
# to check that builds work on small runners. Build output goes to docker volumes, dist/ to the repo's dist/.
#
# Usage: scripts/ci-local.sh linux|windows [memory] [cpus]      e.g. scripts/ci-local.sh windows 2g 2
set -euo pipefail
job="${1:?linux|windows}"
mem="${2:-4g}"
cpus="${3:-2}"
repo="$(cd "$(dirname "$0")/.." && pwd)"

case "$job" in
  linux) inner='
    apt-get update -qq
    apt-get install -y -qq pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libvulkan-dev
    cargo test --workspace --exclude mugon-editor-thumbs --locked --release
    cargo build --release --locked -p mugon-editor -p mugon-editor-thumbs -p mugon-editor-app
    bash scripts/package.sh 0.0.0-local linux-amd64' ;;
  windows) inner='
    export PATH="$CARGO_HOME/bin:$PATH"
    apt-get update -qq
    apt-get install -y -qq clang lld llvm zip
    rustup target add x86_64-pc-windows-msvc
    command -v cargo-xwin >/dev/null || cargo install --locked cargo-xwin
    cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p mugon-editor -p mugon-editor-thumbs -p mugon-editor-app
    bash scripts/package.sh 0.0.0-local windows-amd64' ;;
  *) echo "unknown job: $job" >&2; exit 1 ;;
esac

MSYS_NO_PATHCONV=1 docker run --rm --memory="$mem" --memory-swap="$mem" --cpus="$cpus" \
  -v "$repo:/src" -v "mugon-editor-ci-$job-target:/src/target" -v "mugon-editor-ci-$job-cargo:/cargo" -v "mugon-editor-ci-$job-xwin:/xwin" \
  -e CARGO_HOME=/cargo -e XWIN_CACHE_DIR=/xwin -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=2 -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4 \
  -w /src rust:1 bash -ec "$inner; ls -la dist"
