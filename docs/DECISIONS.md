# Decision records

Short ADR-style log. Newest at the bottom. "Why" is what a future session needs to avoid re-litigating.

## Requirements (from the product owner, interview + follow-ups)

* Tool name: **Mugon Editor** (asset + editor) for the **Mugon** web game platform. Binary `mugon-editor`, crates `mugon-editor-*`.
* Import Synty packs (+metadata), produce glTF with textures split out (one texture per pack, not embedded).
* Browser with filters (name, category, pack, ...), 2D previews, 3D render on click.
* MCP server so agents know what assets exist.
* Optional/second step: editor with fly camera, placing/moving/rotating/scaling models, snapping models next to each
  other, adding components to entities, exporting everything as BSN. Scene description and models should be **separate
  files** (web target).
* Rust preferred over Python. Folder structure of the old `assets/<Pack>/{models,textures,...}` was the only legacy thing
  worth keeping. The library lives **outside** the repo.
* Target Bevy **0.19+**, game ships to web (wasm).
* Skinned meshes / animations are secondary; **no automatic Blender fallback** for FBX2glTF problems.
* Commit at sensible points; keep documentation current (this folder + `PLAN.md`).

## D1: Rust workspace, native Bevy editor launched from the browser
Shared crates between importer, server, MCP and editor; editor in Bevy gives native rendering parity with the game
and a path to native BSN. The browser UI stays plain web (fast to iterate, three.js viewer). Rejected: Go backend,
all-egui UI, wasm editor (more work, no benefit now), three.js editor (would reimplement BSN output).

## D2: `.unitypackage` is the primary input
Prefabs give categories, materials give the exact texture/variant mapping, no atlas guessing. Source-file zips (FBX+PNG)
remain a possible fallback importer (not implemented). See `ARCHITECTURE.md` for the format details.

## D3: Asset = FBX, not prefab
Simple and matches glTF-per-model. Multi-mesh FBX are fine (one glTF, several nodes). Prefabs that combine several FBX
are not composed (rare). Revisit if a pack needs it: a prefab walker would have to read `Transform` hierarchies and
`m_Father` links and emit a glTF/scene with several model nodes.

## D4: No SQLite; in-memory index from `pack.json`
~9k assets filter instantly. Index is rebuilt on start (and on override edits via `/api/reload`). Overrides live in
`overrides.json` so re-imports never lose user tags.

## D5: FBX2glTF stays an external binary
No mature pure-Rust FBX→glTF path. Since ADR 0006 (`docs/ADRs/0006-release-sdks-cli-and-editor.md` in the monorepo) the editor does not ship it:
`mugon_editor_core::fbx2gltf` downloads the pinned upstream release (v0.9.7, SHA-256 checked) into `~/.mugon/tools/fbx2gltf/<version>/` on the first `import`,
because the binary contains Autodesk's proprietary FBX SDK. Known weakness: skinned meshes/animations (accepted, see requirements).

## D6: BSN export = generated Rust `bsn_list!` source
Bevy 0.19 has no `.bsn` asset loader (verified in `bevy_scene-0.19.1/src/lib.rs` docs). The scene description is
therefore compiled into the game; models are separate glTF files referenced by asset path and loaded at runtime. A second
writer can be added if a runtime `.bsn` format ships. Models are referenced as `WorldAssetRoot("path.gltf#Scene0")`
(in 0.19 the former `SceneRoot` is `WorldAssetRoot`, handle type `Handle<WorldAsset>`).

## D7: Editor project file is JSON (`*.mugon-scene.json`), BSN is output only
Avoids needing a BSN parser, stays loss-free for editor-only data. (Originally planned as RON; JSON chosen because
`serde_json` was already in the tree and values for free-form components are JSON anyway.)

## D8: Components are free-form, defined in the game's `mugon.toml`
Not linked to the game crate; fields typed (`bool,i32,f32,string,vec2,vec3,color,enum,asset`). The exporter emits
struct-literal syntax; the game owns the real types. A future option is validating against a registry dumped from the game.

## D9: Snapping = grid/rotation (gizmo) + AABB face snapping
No per-asset sockets needed. Sockets remain a possible extension for modular kits that need precise attachment points.

## D10: Thumbnails by headless Bevy
Same renderer as the game, no Node/Chrome dependency. Uses the manual-pump pattern from Bevy's examples.

## D11: Overrides, not edits, for metadata
Category/tags edited in the browser are stored as overrides keyed by asset id, applied at index load.

## D12: Dev profile uses `debug = 0`
With debuginfo the Bevy editor debug exe was >2 GB and Windows refused to run it ("Exec format error"). Dependencies
are built with `opt-level = 2` in dev for usable speed.

## D13: Distribution via CI (superseded by ADR 0006: GitHub Actions in the mirror, assets on GitHub releases, index on tools.mugon.net)
Tag `v*` builds Windows + Linux archives (all executables + config example; superseded by ADR 0006: GitHub release assets and tools.mugon.net), uploads them to the generic
package registry and creates a release with install scripts (`scripts/install.ps1`, `scripts/install.sh`).

## D14: Library location in `~/.mugon/config.toml` (`[editor]` table)
Resolution order: `--library` / `MUGON_EDITOR_LIBRARY`, then the config file, then (CLI only, interactive terminal) a prompt on
first start that creates the folder and saves it. Non-interactive runs (MCP, CI) fail with a clear message instead of
prompting. `mugon-editor-app` / `mugon-editor-thumbs` read the config but never prompt; `mugon-editor` (no subcommand) and `mugon-editor thumbs` pass `--library`
explicitly. Implemented in `mugon-editor-core::config`. Home dir comes from `USERPROFILE`/`HOME` (no extra crate).

## D15: Import skips existing packs; thumbnails are gitignored
`mugon-editor import` skips a pack whose `pack.json` exists (`--force` re-imports), so folder imports are incremental.
Every import (and `mugon-editor thumbs`, for older libraries) writes `<pack>/thumbnails/.gitignore` (`*.png`) via
`mugon_editor_core::ensure_thumbnails_dir`, so a library under version control stays small.

## Insights worth remembering

* Unity `.mat` base texture property differs per shader: `_MainTex` (Simple), `_Albedo_Map` (Polygon shader graph).
* Synty FBX `globalScale` is 100 in the `.meta`; FBX2glTF already applies the cm→m 0.01, so the importer multiplies the
  root node scale by `globalScale` (otherwise a car is 3 cm long).
* FBX2glTF `-o name` writes `name_out/name.gltf` + `buffer.bin`.
* Polygon packs ship TGA textures; convert to PNG.
* Many Polygon assets are `FX_*`, `*_Collision`, decals, planes; the editor palette hides category `Collision`.
* `transform-gizmo-bevy` adds per-frame deltas to the current transform (see ARCHITECTURE: why snapping recomputes from the drag start).
* bevy_egui needs its own camera when the 3D camera has a custom viewport.
* Crate compatibility for Bevy 0.19: `bevy_egui 0.42`, `transform-gizmo-bevy 0.11` (0.43 / 0.10 target other Bevy versions).

## D16: Markers = unit volumes scaled by the transform
A marker is a project entity with `marker: Box | Sphere | Cylinder` and no model. It is a **unit** shape (1 m box, sphere
of diameter 1, cylinder of diameter 1 / height 1, centred on the origin) and its **size is the entity's `Transform.scale`**,
so the scale gizmo resizes it, picking/neighbour snapping reuse the unit bounds, and the exported `Transform` alone
describes the space it occupies. Export emits `Transform { .. }` + `Marker { shape: MarkerShape::<Shape> }` + any user
components; `Marker`/`MarkerShape` live in `mugon_sdk_bevy::scene` (exported scenes import them) and games read the extents with
`mugon_sdk_bevy::scene::markers`. Consequence: avoid parenting children to a non-uniformly scaled marker (they inherit
the scale). Editor view: translucent unlit material + wireframe outline (`markers.rs`). Alternatives rejected: separate size
field (two sources of truth for size), Bevy `Aabb` component (it is a culling component).

## D17: Custom runtime scene format (not implemented, feasibility notes)
Possible and endorsed by Bevy's docs: a game-side plugin registers an `AssetLoader` returning `ScenePatch`/`SceneListPatch`
(`Box<dyn Scene>`), whose `resolve` pushes type-erased templates (`ResolvedScene::push_template_erased`, `SceneFunction`).
Plan if pursued: Bevy-free schema in `mugon-editor-scene` (text RON/JSON + compact binary), explicit registry
`app.register_scene_component::<T>("Name")` keyed by the `[[editor.component]]` names (preferred over reflection), models as
path handle templates with asset dependencies, hot reload via the AssetServer. Risks: unstable scene API per Bevy release,
official `.bsn` is planned but unscheduled (checked 2026-10-07: not in the 0.19 release, no dedicated 0.20 milestone item;
the 0.20 migration guide only changes `bsn!` macro syntax). Keep the schema BSN-shaped so a later `.bsn` writer is a swap.

## D18: Unity-style viewport interaction (2026-10-07)
Palette items are dragged into the viewport (egui drag -> `Settings.drop`, placed by `viewport_click` with the same
ground/model-hit + snapping logic as before); click-to-arm only remains for `--asset`. Left press/release in the viewport
is one gesture: below 4 px = click select/deselect, above = marquee (projected world AABB overlap, drawn by `ui_system`).
Tools stay separate like Unity (`W` move, `E` rotate, `R` scale; a universal gizmo was tried and dropped) with a button overlay on the viewport's right edge; tool keys are ignored while the right mouse button flies the camera. Undo/redo match the typed letter (`ButtonInput<Key>`), not the physical `KeyCode`, because Z/Y swap on QWERTZ keyboards. Arrow-key nudges repeat while held (0.35 s delay, 20/s) as one undo step. Copy/paste is an
in-app `SceneClipboard` of subtrees (`Project::copy_subtrees`/`paste`); arrow keys nudge via `Ctx::nudge_selected`.
Untested interactively (see DEVELOPMENT.md): drag-drop and marquee selection need a manual run.

## D-pick: click picking uses mesh ray casts, not bounding boxes
`interact::pick` uses Bevy's `MeshRayCast` (always-early-exit, filtered to meshes under an `EditorId`), so empty space
inside a model's box no longer selects it and drop placement snaps to the real surface. Backfaces are culled (Bevy
default), so single-sided geometry is not pickable from behind. Entities still loading (no `Mesh3d`, no `Children`) are
picked by OBB so a fresh drop is clickable immediately. Marquee selection and neighbour snapping still use AABBs.
Not driven interactively (see DEVELOPMENT.md).

## D16: The editor is configured from the game's `mugon.toml`
Components (`[[editor.component]]`), scene folders (`[editor.scenes] json-path`, `bsn-path`, `bsn-use`) and the asset folder
(`[editor.assets] path`) live in the game's `mugon.toml`; the library path is global (`[editor] library` in `~/.mugon/config.toml`).
Rationale and the full set of decisions: `docs/ADRs/0004-configure-the-editor-from-the-game-project.md` in the monorepo root.
