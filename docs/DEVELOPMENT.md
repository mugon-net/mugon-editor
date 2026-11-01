# Development guide

## Build and test

```
cargo build --release                 # all executables: mugon-editor, mugon-editor-thumbs, mugon-editor-app
cargo test --workspace                # unit + golden tests (builds Bevy once for mugon-editor-app)
UPDATE_GOLDEN=1 cargo test -p mugon-editor-scene   # regenerate crates/mugon-editor-bsn-check/src/level.rs after changing the exporter
cargo test --manifest-path crates/mugon-editor-bsn-check/Cargo.toml   # compile + spawn the golden BSN (monorepo only)
```

`mugon-editor-bsn-check` is excluded from the workspace and has its own `Cargo.lock`: it depends on `../../../mugon-sdk-bevy` by path
(exported scenes import `mugon_sdk_bevy::scene::{Marker, MarkerShape}`), which does not exist in the GitHub mirror of this folder.
The monorepo CI job `mugon-editor:bsn-check` runs it.

Toolchain: current stable Rust (developed with 1.98). Bevy 0.19.x, `bevy_egui 0.42`, `transform-gizmo-bevy 0.11`.
The dev profile has `debug = 0` (see D12 in `DECISIONS.md`); first Bevy builds take 5-10 minutes.

Linux build dependencies (also used by CI): `pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev
libxkbcommon-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libvulkan-dev`.

## Running against a library

The test library used during development was `E:/asitor-library` (outside the repo; pass `--library` or `mugon-editor config --set-library`). To test config/first-run behaviour without touching your real config set `USERPROFILE` (Windows) / `HOME` to a scratch folder. Typical loop:

```
mugon-editor --library <lib> import import/            # all packages in ./import (gitignored)
mugon-editor --library <lib> thumbs [--pack P] [--filter text] [--force]
mugon-editor --library <lib> serve                     # http://127.0.0.1:7878
mugon-editor-app --library <lib> --demo --show-help --screenshot out.png   # renders a demo scene and exits (visual smoke test)
mugon-editor-app --library <lib> --smoke 120                    # run 120 frames and exit
```

Import takes ~2.5 min for 9 packs; thumbnails ~3 min for ~9000 assets. Both are idempotent (import rebuilds a pack's
`models/`, `textures/` and `pack.json`; thumbnails skip existing PNGs unless `--force`). After re-importing a pack, run
`thumbs` again (the import resets `thumbnail` in `pack.json`; existing PNGs are re-linked, not re-rendered, unless
`--force`; use `--force` when the model/texture changed).

## What is and is not verified

Automated: importer helpers (pack ids, tags, categories, material parsing), scene model (remove/duplicate/save), component
registry, BSN export + bundling (golden file compiled and spawned in real Bevy), snapping/ray math, undo/redo logic.

Manually verified during development: import of 9 packs, REST API, browser UI + 3D viewer in Chrome, MCP over stdio
(initialize / tools list / call), thumbnails visually, editor rendering via `--demo --screenshot`.

**Never verified by driving the window**: mouse picking, gizmo dragging, neighbour snapping feel, camera controls, file
dialogs, export dialog, the browser's "Open in editor" button. A session that can drive a desktop app should do a pass
over these first. The CI pipeline has not been run either.

## Conventions

* Keep Bevy out of the non-Bevy crates. Anything an agent or the CLI needs lives in `mugon-editor-core`/`mugon-editor-scene`.
* Stored paths use `/` and are relative to their pack folder (fix Windows backslashes at the boundary).
* Add an ADR line to `docs/DECISIONS.md` for any choice that is hard to reverse; update `PLAN.md` status when features land.
* Commit messages: short imperative subject, trailer lines as configured by the session.

## Release / distribution

Releases are built by the GitHub Actions workflow `.github/workflows/main.yaml` of the `mugon-net/mugon-editor` mirror (see
`docs/ADRs/0006-release-sdks-cli-and-editor.md` in the monorepo): `test` on every push/PR; a manual `workflow_dispatch` run with
`version` (checked against the workspace version), `requires-cli` and `dry-run` builds Linux amd64 and the cross-compiled Windows amd64
archives (`scripts/package.sh <version> <platform>` -> `dist/mugon-editor-<version>-<platform>.{tar.gz,zip}` with the three executables,
`README.md` and `config/mugon.example.toml`), creates the GitHub release and `scripts/publish-tools.sh` adds the version to
`https://tools.mugon.net/editor/index.json` (stable versions only) next to the install scripts. The Windows build is cross-compiled to
`x86_64-pc-windows-msvc` with `cargo-xwin` (needs `clang lld llvm zip`; downloads the Microsoft CRT/SDK on first use).
Bump the version with `node libraries/scripts/bump-version.mjs editor <x.y.z>` in the monorepo.
Low-RAM runners (2-4 GB): the build is CPU-bound (~15 min cold on 2 cores); `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`,
Bevy with only the `3d` feature set and `strip = "symbols"` keep it within 4 GB.
Reproduce locally with `scripts/ci-local.sh linux|windows [memory] [cpus]` (docker; mirrors the CI jobs, defaults to 4g / 2 CPUs).
To cut a release: bump the version in the monorepo, wait for the mirror sync, then run the workflow (dry run first).

## Ideas not done yet

* Prefab compositions (several FBX in one prefab) as one asset; source-zip importer fallback.
* Apply texture variants in editor/export (needs per-entity material override or per-variant glTF).
* Bundle optimisation (meshopt, KTX2, texture downscale) for smaller web builds.
* Socket-based snapping for modular kits; terrain/paint tools.
* Validate exported component names/fields against a registry dumped from the game (e.g. via BRP).
* Runtime `.bsn` writer once Bevy ships the format.
