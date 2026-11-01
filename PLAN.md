# Mugon Editor: plan and status

**Mugon Editor** (asset + editor) is the asset library and scene editor for the Mugon web game platform: import Synty
packs, browse them, expose them to agents (MCP), and build Bevy 0.19 scenes that export as BSN.

Companion documents: `README.md` (usage), `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` (requirements, ADRs, insights),
`docs/DEVELOPMENT.md` (build, test, verification status, release).

## Requirements (summary; full list in `docs/DECISIONS.md`)

1. Auto-import Synty packs (+metadata) → glTF with textures split out (one shared texture per pack).
2. Browser with filters (name, category, pack, tags…), 2D previews, 3D render on click.
3. MCP server describing all available models to an agent.
4. Editor (second step): camera navigation, place models, move/rotate/scale, snap models next to each other, add
   components to entities, export everything as BSN. Scene description and models are separate files (web target).
5. Rust, library outside the repo, Bevy 0.19+, web game target. Skinned/animated assets are secondary; no automatic Blender fallback.
6. Distribution: GitHub Actions builds executables, users can download/install easily.

## Status

| Area | State |
|---|---|
| Importer (`.unitypackage`) | done; 9 packs / ~9000 assets imported without failures |
| Browser UI + REST | done, verified in Chrome |
| Thumbnails (headless Bevy) | done, all assets rendered |
| MCP server | done, verified over stdio |
| Scene model, components config, BSN export, bundler | done, golden-tested against real Bevy 0.19 |
| Editor | implemented; only verified by screenshot (`--demo --screenshot`) and unit tests, **interactive behaviour untested** |
| Markers | done: box/sphere/cylinder volume placeholders (translucent in editor, size = scale), exported as `Transform` + `Marker { shape }` + user components |
| CLI usability | done: saved library config (`[editor]` in `~/.mugon/config.toml`, first-run prompt), incremental folder import, bare `mugon-editor` opens the editor, `mugon-editor config`, in-editor shortcut hint (F1), thumbnails `.gitignore` |
| CI / install scripts | written; packaging script and installed-layout verified locally, **pipeline never run** |

Key finding: **Bevy 0.19 has no `.bsn` loader**, so BSN export = generated Rust `bsn_list!` source with models as
separate glTF files (ADR D6).

## Open items / roadmap

Verification (highest value next):
- [ ] Drive the editor interactively: picking, placing, gizmo drags, neighbour snapping, camera, dialogs, export dialog,
      "Open in editor" from the browser. Expect tuning (snap threshold, camera speeds, hotkeys).
- [ ] Run the GitHub release workflow once (dry run, then a real `v0.1.0`) and fix runner/package issues; verify `mugon editor install` and the install scripts (ADR 0006).
- [ ] Load an exported scene in a real Bevy 0.19 game with the bundled assets, in a wasm build.

Features:
- [ ] Prefab compositions (several FBX in one prefab; ~5% of Polygon_War prefabs, 0% in Simple_Military) as one asset.
      Note: a single FBX with several meshes (e.g. `SK_Veh_Tank_01`) already imports completely as one glTF.
- [ ] Source-file zip importer fallback (FBX + PNG zips, as downloaded originally).
- [ ] Texture variants in the editor/export (per-entity material override or per-variant glTF).
- [ ] Bundle optimisation for web: meshopt, KTX2/texture downscale, glb packing.
- [ ] Per-asset snap sockets for modular kits; multi-select editing in the inspector; entity grouping tools.
- [ ] Validate exported components against a registry dumped from the game.
- [ ] Runtime scene format: a custom data format + game-side loader plugin is feasible on Bevy 0.19 (`ScenePatch` assets via a custom `AssetLoader`; see the discussion in `docs/DECISIONS.md` D17). Official `.bsn` is still unscheduled.
- [ ] **Bevy 0.20 migration** (0.20.0-rc.2 was out on 2026-10-07, final not yet released): the `bsn!` syntax changes (scene references use `@`, entities separated with `--`, `{}` preferred over `[]`/`()` in `bsn_list!`, no `template_value()`); the exporter output and the golden file in `mugon-editor-bsn-check` will need updating, and `bevy_egui`/`transform-gizmo-bevy` versions must follow.
- [ ] Skinned meshes / animation quality via FBX2glTF is known to be imperfect (accepted).
- [ ] Known data gaps: some `FX_*`/decal assets have no texture; sub-part FBX (e.g. `tank_01_body`) show up as extra assets.

## Design outline: custom runtime scene format (NOT implemented)

Goal: instead of (or in addition to) compiling the scene into the game as generated `bsn_list!` Rust, export a **data file**
that a game plugin loads at runtime and turns into a BSN scene. Benefits: hot reload (re-export, the game reloads without a
recompile), smaller wasm (scene data is not code), no dependence on `bsn!` macro syntax churn (0.20 changes it), scenes can be
streamed/loaded per area. Feasibility and background: ADR D17 in `docs/DECISIONS.md`.
Bevy hook: a custom `AssetLoader` returns `ScenePatch` / `SceneListPatch` (`Box<dyn Scene>` / `Box<dyn SceneList>`); the
returned scene's `resolve` pushes templates into a `ResolvedScene` (`push_template_erased`, `SceneFunction`). Because those
are real scene assets, `world.spawn_scene_list(handle)` and `bsn!{ :"level.<ext>" ... }` inheritance work like with the
future official `.bsn`.

### Crates
* `mugon-editor-scene` (existing, Bevy-free): add `data` module with the schema + serialisers (and the reader, used by tests and tools).
* **new `mugon-editor-bevy`** (game-side plugin, depends on Bevy, published/vendored into the game): `MugonEditorScenePlugin`, the
  `AssetLoader`, the component registry, `Marker`/`MarkerShape` types (moved here from the `mugon-editor-bsn-check` reference
  definition so games stop copy-pasting them). One adapter per supported Bevy version (the scene/template API is new and
  changes between releases); the schema crate does not depend on Bevy.
* CLI: `mugon-editor export <scene> --format rust|data` (default `rust`; both may be emitted together), `--out`, same `--prefix`,
  `--bundle` behaviour (bundling already works from the project, independent of the format).

### Schema (version 1), Bevy-free and BSN-shaped
```
SceneFile { format: "mugon-editor-scene", version: 1, name, roots: [Entity] }
Entity    { name, transform?: {t:[3], r:[4], s:[3]}, model?: "<prefix><pack>/models/X/X.gltf#Scene0",
            marker?: Box|Sphere|Cylinder, components: [ { type: "Health", fields: { max: 40.0, ... } } ],
            children: [Entity], inherits?: "other.scene" }
```
* Same information `export_bsn` walks today; `export_bsn` and the data writer should share one intermediate walk (`ExportTree`)
  so the two outputs cannot drift. Keep the names BSN uses (patch, inherit, name reference) so a future official `.bsn`
  writer is a swap, not a redesign.
* Field values reuse the editor's JSON values typed by `components.toml` (`bool,i32,f32,string,vec2,vec3,color,enum,asset`).
  Enums by variant name; colours as sRGBA floats; `asset` fields as asset-path strings.
* Encodings, selected by file extension: text (`.mugon-editor-scene.ron` or `.json`, diffable, default in dev) and binary
  (`.mugon-editor-scene.bin`, `postcard` or `bitcode`, optionally zstd; for the web build). Version header first; unknown major
  version = error.
* Not representable (by design): closures/observers, arbitrary expressions. If ever needed, refer to game-registered
  named handlers by string.

### Loader (`mugon-editor-bevy`)
1. Parse the file (by extension) into `SceneFile`; validate version.
2. Build a `DataScene` (impl `Scene`/`SceneList`) from the tree. Its `resolve` creates the entity, pushes:
   `Name`, `Transform` (when non-default), `WorldAssetRoot` as a **handle template from the path string** (so the glTF is an
   asset dependency of the scene asset: `register_dependencies` / load state waits for models), `Marker`, user components,
   and children through the related-scenes mechanism (`ChildOf`/`Children`).
3. User components via an **explicit registry** (chosen over reflection, ADR D17):
   `app.register_scene_component::<Health>("Health")` for `T: Component + Clone + Default + DeserializeOwned`
   (a `Template` is free for `Default + Clone`). The key equals the name in `components.toml`, so the editor config stays the
   single contract. Handle-typed fields use an `AssetField<T>` newtype / handle template resolved from the path at spawn time.
   Built-in registrations: `Transform`, `Name`, `Marker`. A `register_scene_component_with(name, fn(&Value) -> Template)`
   escape hatch handles odd types.
4. Unknown component or field: hard error in debug builds, `warn!` + skip in release (configurable).
5. `inherits` maps to `CachedSceneAsset` (`:"other.scene"`), so scenes can compose.
6. Entity-to-entity references (e.g. a component field pointing at another entity by name) are **not** part of v1; the
   editor does not author them. Extension: `{ "$ref": "Name" }` field value resolved with `EntityTemplate` in a second pass.

### Editor / exporter changes
None to the editor. `mugon-editor-scene`: data writer + reader + round-trip tests; `export` dialog gets a format choice; `components.toml`
may gain an optional per-field `rust_type` (already used for enums) that the registry can check against.

### Testing
* Round-trip: `Project` -> `SceneFile` -> text/binary -> `SceneFile` equality; golden files checked in.
* Equivalence in `mugon-editor-bsn-check` (extended): spawn the generated-Rust scene and the loaded data scene in a headless
  `App`, compare entity count, `Name`s, `Transform`s, `Marker`s and component values. This is the guard against the two
  exporters diverging.
* Hot-reload test: modify the file in a test asset dir, assert the spawned scene updates.
* wasm: build `mugon-editor-bevy` for `wasm32-unknown-unknown`, check binary size and parse time of a large scene.

### Phases
1. Schema + shared `ExportTree` + text writer/reader + round-trip tests (Bevy-free, ~0.5 day).
2. `mugon-editor-bevy` loader for Transform/Name/model/children/Marker + equivalence test (~1 day).
3. Component registry incl. handle fields + error policy + `--format data` in CLI/editor (~1 day).
4. Binary encoding + zstd + size benchmark; `inherits`; entity references (optional).
5. Port to Bevy 0.20 together with the `bsn!` syntax migration (separate adapter module or feature flag).

### Open questions
* Registry vs reflection (default: registry), whether to publish `mugon-editor-bevy` or vendor it into the game.
* Single combined file for one game level vs one file per "room/area" (streaming), and how `--bundle` should group assets then.
* Whether the data format should also carry the editor-only data (selection, camera) — no, keep the project file for that.
* Keep both outputs long term, or drop the generated-Rust path once the loader is trusted (and once Bevy ships `.bsn`, drop ours).

## History

Built in one session from the original Python/Node prototype (discarded): importer → browser → thumbnails → MCP → scene/export →
editor → docs/CI. The old committed `assets/` output, Python scripts and Next.js template were removed.
