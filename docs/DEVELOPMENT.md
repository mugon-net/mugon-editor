# Development guide

## Build and test

```
cargo build --release                 # all executables: mugon-editor, mugon-editor-thumbs, mugon-editor-app
cargo test --workspace                # unit + golden tests (builds Bevy once for mugon-editor-app / mugon-editor-bsn-check)
UPDATE_GOLDEN=1 cargo test -p mugon-editor-scene   # regenerate crates/mugon-editor-bsn-check/src/level.rs after changing the exporter
```

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
over these first. The CI pipeline has not been run on a real GitLab instance either.

## Conventions

* Keep Bevy out of the non-Bevy crates. Anything an agent or the CLI needs lives in `mugon-editor-core`/`mugon-editor-scene`.
* Stored paths use `/` and are relative to their pack folder (fix Windows backslashes at the boundary).
* Add an ADR line to `docs/DECISIONS.md` for any choice that is hard to reverse; update `PLAN.md` status when features land.
* Commit messages: short imperative subject, trailer lines as configured by the session.

## Release / distribution

`.gitlab-ci.yml` stages: `test` -> `build` (Linux x86_64 + Windows x86_64) -> `release` (only on tags `v*`).
Both build jobs run on **Linux** (docker) runners; the Windows build is cross-compiled to `x86_64-pc-windows-msvc` with
`cargo-xwin` (needs `clang lld llvm zip`; downloads the Microsoft CRT/SDK into `XWIN_CACHE_DIR` on first use). Each job runs
`scripts/package.sh <version> <platform>` (`scripts/package.ps1` is only for packaging a local Windows build), producing `dist/mugon-editor-<version>-<platform>.{tar.gz,zip}`
containing the three executables, `FBX2glTF`, `README.md` and `config/mugon.example.toml`.
The release job uploads the archives to the project's generic package registry
(`.../packages/generic/mugon-editor/<version>/`), creates a GitLab release with links to them, and publishes
`install.ps1` / `install.sh` (with the project API URL/ID substituted) as release links.
CI build cache (the Bevy dependency tree is ~20 min cold): one rolling cache key per platform
(`linux-release-$CACHE_VERSION`, `windows-xwin-$CACHE_VERSION`), saved even when a job fails, with the `test` job running
`cargo test --release` so test and build jobs share compiled dependencies; the Windows job also caches `cargo-xwin` (`.cargo/bin`) and the downloaded SDK (`.xwin-cache`).
Caches are per runner unless a distributed cache is configured (gitlab.com shared runners have one; self-hosted runners
need S3/GCS cache config). Bump `CACHE_VERSION` in `.gitlab-ci.yml` to reset caches if they grow too large (cargo never prunes
old artifacts from `target`). 
**Low-RAM runners (2-4 GB).** Verified locally with `docker run --memory=4g --memory-swap=4g --cpus=2 rust:1` and a cold cache:
the Linux test + build jobs (~14.7 min total) and the cross-compiled Windows build (~17 min incl. installing cargo-xwin) both
succeeded without OOM kills. What makes that work: `CARGO_BUILD_JOBS=2` (limits parallel rustc processes; use 1 for 2 GB
runners, 0 = all cores on big runners), `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`, Bevy built with only the `3d` feature set for
`mugon-editor-app` / `mugon-editor-thumbs` (no 2d/ui/audio crates), the test job excluding `mugon-editor-thumbs` (one fewer 150 MB Bevy
link), and `[profile.release] strip = "symbols"` (archives ~55-60 MB instead of ~110 MB). Note that on small runners
the time is mostly CPU-bound (2 cores), not RAM-bound: RAM only decides between "builds" and "OOM-killed/swapping".
Cold builds are slow by nature (~15 min on 2 cores); the rolling caches are what make later pipelines fast.
Reproduce locally with `scripts/ci-local.sh linux|windows [memory] [cpus]` (docker; mirrors the CI jobs, defaults to 4g / 2 CPUs).
To cut a release: `git tag v0.1.0 && git push origin v0.1.0`.

## Ideas not done yet

* Prefab compositions (several FBX in one prefab) as one asset; source-zip importer fallback.
* Apply texture variants in editor/export (needs per-entity material override or per-variant glTF).
* Bundle optimisation (meshopt, KTX2, texture downscale) for smaller web builds.
* Socket-based snapping for modular kits; terrain/paint tools.
* Validate exported component names/fields against a registry dumped from the game (e.g. via BRP).
* Runtime `.bsn` writer once Bevy ships the format.
