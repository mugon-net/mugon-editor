# Architecture

Mugon Editor is a Rust workspace. Everything shares the on-disk **library** (a folder outside the repo) and the
in-memory index built from it.

```
.unitypackage ──import──▶ library/<Pack>/{pack.json, models/, textures/, thumbnails/}
                               │
        ┌──────────────────────┼───────────────────────────┬──────────────┐
        ▼                      ▼                           ▼              ▼
   mugon-editor serve           mugon-editor mcp                 mugon-editor-thumbs   mugon-editor-app ──▶ scene.mugon-scene.json
 (REST + web UI)        (agent tools, stdio)       (2D previews)           │
                                                                           ▼
                                                      mugon-editor export ──▶ level.rs (bsn_list!) + bundled glTFs
```

## Crates

| Crate | Role | Bevy? |
|---|---|---|
| `mugon-editor-core` | Data model (`Pack`, `Asset`, `Variant`, `Bounds`), `Library` (load packs), `index` (search, facets, `overrides.json`). | no |
| `mugon-editor-import` | `.unitypackage` reader, Unity YAML scanning, FBX→glTF (external FBX2glTF), glTF post-processing, naming/category heuristics. | no |
| `mugon-editor-server` | axum REST API, serves library files at `/lib/*` and the embedded web UI (`crates/mugon-editor-server/ui`, vanilla JS + vendored three.js). | no |
| `mugon-editor-mcp` | MCP server over stdio (`rmcp`), read-only tools over the index. | no |
| `mugon-editor-scene` | Editor project model (`Project`, `SceneEntity`), the game config (`[editor]` table of `mugon.toml`: components, folders), BSN code generation, asset bundler. | no |
| `mugon-editor-thumbs` | Headless Bevy renderer writing `thumbnails/<name>.png`, updates `pack.json`. | yes |
| `mugon-editor-app` | Bevy + egui + `transform-gizmo-bevy` scene editor. | yes |
| `mugon-editor-bsn-check` | Test-only crate: compiles the golden generated BSN against real Bevy 0.19 and spawns it. | yes |
| `mugon-editor` | The CLI that ties the above together (`import`, `list`, `thumbs`, `serve`, `mcp`, `export`). | no |

Bevy-free crates stay light so that importer/exporter work and tests do not need a Bevy build.
`mugon-editor thumbs` and the browser's "Open in editor" button start the sibling `mugon-editor-thumbs` / `mugon-editor-app`
executables (looked up next to the `mugon-editor` binary), so the main CLI never links Bevy.

## Library on disk

Set with `--library`, `MUGON_EDITOR_LIBRARY` or `[editor] library` in `~/.mugon/config.toml` (see D14). Not part of the repo. All stored paths use `/` and are relative to the pack folder.

```
<library>/
  overrides.json           # user edits: { "<pack>/<asset>": { category?, add_tags[], remove_tags[], hidden } }
  <Pack>/                  # e.g. Polygon_War, Simple_Racer  (id derived from the package file name)
    pack.json              # { id, name, family, source, textures[], assets[] }
    textures/*.png         # shared atlases + colour variants, stored once per pack
    models/<Asset>/<Asset>.gltf + buffer.bin   # glTF references ../../textures/<atlas>.png
    thumbnails/<Asset>.png + .gitignore (*.png)
```

`Asset` (see `crates/mugon-editor-core/src/lib.rs`): `id` (`<pack>/<name>`), `pack`, `name`, `kind` (`static|skinned`),
`category` (may contain `/`), `tags`, `model`, `texture` (default atlas), `variants[{name,texture}]`, `bounds{min,max}`
(meters, includes Unity unit scale), `triangles`, `animations[]`, `thumbnail`.
`overrides.json` is applied when the index loads and is never touched by re-imports.

## Importer (`mugon-editor-import`)

A `.unitypackage` is a tar.gz of `<guid>/{asset, asset.meta, pathname, preview.png}`. The importer:

1. Extracts every `asset` to a temp dir (`<library>/.tmp`, removed afterwards) and reads all `pathname`s and `asset.meta`s.
2. Parses materials (`.mat`): the base texture property is one of `_MainTex, _BaseMap, _BaseColorMap, _Albedo_Map, ...`
   (Polygon shader-graph materials use `_Albedo_Map`).
3. Parses prefabs (`.prefab`, Unity YAML, split on `--- !u!<class> &<id>`): `MeshFilter` (33),
   `MeshRenderer` (23), `SkinnedMeshRenderer` (137) give mesh + material guids; **`PrefabInstance` (1001)** gives the
   source FBX (`m_SourcePrefab`) and material overrides (`propertyPath: m_Materials.Array.data[N]`). Most Polygon
   prefabs are instances, so this matters.
4. **Asset = FBX file.** Category = most common prefab folder below `Prefabs/` that references it (fallback: name token
   like `Prop`, `Bld`, `Veh`). Default texture = most used texture among the prefabs' materials; other textures =
   `variants`.
5. Converts each FBX with FBX2glTF (parallel via rayon), then post-processes the glTF JSON: drops the placeholder
   embedded image, references the shared pack texture, sets simple PBR defaults (metallic 0, roughness 1, opaque),
   multiplies the root node scale by Unity's `globalScale` import setting (FBX2glTF already converts cm→m, Synty FBX use
   `globalScale: 100`), computes bounds, triangle count, skin/animation info.
6. Textures other than png/jpg (TGA) are converted to PNG (browsers and the web game cannot read TGA).

A model that is one FBX with several meshes/nodes (e.g. `SK_Veh_Tank_01`: body, turret, barrel, lids, MG) is imported
completely as one glTF with several nodes. What is *not* handled is a prefab that combines several different FBX
files into one object (about 5% of Polygon_War prefabs, none in Simple_Military); those FBX appear as separate assets.

## Thumbnails (`mugon-editor-thumbs`)

Bevy 0.19 driven manually (no winit, no window): `RenderPlugin` with synchronous pipeline compilation, a render-target
`Image`, `Screenshot::image(...)` + `save_to_disk`, and the update loop pumped by hand with a device poll each frame
(pattern from Bevy's `externally_driven_headless_renderer` example). Per asset: load the glTF via
`WorldAssetRoot`, wait for `is_loaded_with_dependencies`, a few frames, screenshot, despawn. The camera is framed from
`bounds`. Roughly 15 ms per asset. Assets that never finish loading are skipped after 600 frames.

## Web UI and REST (`mugon-editor-server`)

Static UI is embedded with `rust-embed` (so the single `mugon-editor` binary serves it). Endpoints:
`GET /api/info|packs|categories|tags|assets|asset`, `POST /api/override|reload|editor/open`, `GET /lib/*` (library files).
The 3D viewer loads the glTF directly (its relative texture URI resolves because `/lib` mirrors the library layout) and
swaps textures for variants.

## MCP (`mugon-editor-mcp`)

stdio server with tools `get_library_summary`, `list_packs`, `list_categories`, `list_tags`, `search_assets`,
`get_asset` (absolute model path, texture, variants), `find_similar`. Read-only. Register with an MCP client using
command `mugon-editor` and args `--library <dir> mcp`.

## Editor (`mugon-editor-app`)

* **Source of truth**: `mugon_editor_scene::Project` (resource `EditorProject`), saved as `*.mugon-scene.json`. Bevy entities are a
  view: `scene_sync::reconcile` (triggered by `Settings::rebuild`) spawns/despawns/updates entities, hierarchy
  (`ChildOf`), models (`WorldAssetRoot`) and `LocalBounds` from the project.
* While a gizmo drag is active the Bevy `Transform` is authoritative and is copied back (`transforms_to_project`);
  the undo snapshot for a drag is taken on the first active frame.
* **Undo/redo** = whole-project snapshots (`History`). Inspector edits are coalesced into one step (0.6 s window).
* **UI**: bevy_egui. egui renders through its **own** `Camera2d` (`PrimaryEguiContext`, auto-creation disabled); the
  3D camera's `Viewport` is set to the free area between panels (otherwise egui gets squeezed into that viewport).
* **Picking/placing**: triangle-accurate `MeshRayCast` against the loaded glTF meshes (`interact::pick`); each hit mesh
  is mapped to its entity via the nearest `EditorId` ancestor. Entities whose scene has not spawned yet fall back to
  ray vs oriented bounding box (`geom::ray_obb`). Placing hits the nearest of ground plane y=0 and model surfaces.
* **Snapping**: gizmo's own grid/angle/scale snapping, plus neighbour snapping (`geom::snap_translation`): per axis it
  aligns/touches faces of nearby AABBs within a threshold. `transform-gizmo-bevy` accumulates per-frame deltas on the
  current transform, which would make a snapped object "sticky", so `interact::face_snap_drag` recomputes the position every
  frame from `drag start + GizmoResult::Translation.total + snap`.
* **Markers** (`markers.rs`, ADR D16): entities with `SceneEntity.marker` set; shown as a translucent unit mesh + gizmo outline, `LocalBounds` = unit box so picking/snapping work unchanged.
* Components are free-form (`[[editor.component]]` in the game's `mugon.toml`), stored as JSON values per entity.

## Export (`mugon-editor-scene::export`)

Bevy 0.19 has no `.bsn` file format/loader (the docs say it is not released yet); BSN is the `bsn!`/`bsn_list!` macro.
So `export_bsn` generates Rust source:

```rust
pub fn level_one() -> impl SceneList {
    bsn_list![ ( #Name Transform { translation: Vec3::new(..) } WorldAssetRoot("synty/Pack/models/X/X.gltf#Scene0")
                 Health { max: 40.0 } Children [ ... ] ), ]
}
```

`#Name` is the sanitised, de-duplicated entity name; transform fields are only emitted when not default; component
field values are rendered by type (`String::from("..")`, `Vec3::new`, `Color::srgba`, enum as `<rust_type>::<Variant>`).
The game must have those component types in scope (`--use crate::components::*`) and register a `Default + Clone`
(or `FromTemplate`) impl. `bundle` copies the used glTFs, their buffers and the default texture into the game asset
directory keeping the `<prefix><pack>/...` layout so glTF-relative texture paths keep working, and writes `manifest.json`.

Correctness is guarded by a golden file: `mugon-editor-scene`'s export test compares against
`crates/mugon-editor-bsn-check/src/level.rs`, which that crate compiles and spawns. Regenerate with
`UPDATE_GOLDEN=1 cargo test -p mugon-editor-scene`.
