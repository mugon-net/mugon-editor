# Mugon Editor

**Mugon Editor** is the asset library and scene editor for the **Mugon** web game platform. It turns Synty asset packs into a browsable
glTF library, tells AI agents what is in it, and lets you build Bevy scenes from the models and export them as BSN.

* **Importer**: Synty `.unitypackage` → glTF per model (one shared texture per pack, not embedded), categories, colour variants.
* **Browser**: web UI with search, pack/category/tag filters, thumbnails and a 3D viewer.
* **MCP server**: lets an agent search and inspect the library.
* **Editor**: native Bevy app. Place models, move/rotate/scale with gizmos, snap to grid or to neighbouring models, attach
  components, save scenes, export BSN (Bevy 0.19) plus the used model files.

## Install

Download the latest archive from the project's **Releases** page (Windows `.zip` or Linux `.tar.gz`), or use the installer:

```powershell
# Windows (PowerShell)
irm <release-link-to-install.ps1> | iex
```
```bash
# Linux
curl -fsSL <release-link-to-install.sh> | bash
```

Private project? Set `GITLAB_TOKEN` (a token with `read_api`) first. The archive contains `mugon-editor`, `mugon-editor-thumbs`,
`mugon-editor-app`, `FBX2glTF` and `config/mugon.example.toml`; keep them in one folder. The install scripts put them in `~/.mugon/editor`
(`%USERPROFILE%\.mugon\editor` on Windows); `mugon editor` from the Mugon CLI runs this `mugon-editor`. The editor and thumbnail
renderer need a GPU with Vulkan/DX12 drivers.

Build from source instead: `cargo build --release` (Rust stable; Linux needs the packages listed in `docs/DEVELOPMENT.md`).

## Quick start

The first time you run any `mugon-editor` command that needs the library, it asks for a **library folder** (outside this repo,
it gets large) and saves it to the `[editor]` table of `~/.mugon/config.toml` (`%USERPROFILE%\.mugon\config.toml` on Windows; the file the Mugon CLI uses too):

```toml
[editor]
library = "D:/asitor-library"
```

All tools (`mugon-editor`, `mugon-editor-app`, `mugon-editor-thumbs`) read it from there. Override per call with `--library <dir>` or
`MUGON_EDITOR_LIBRARY`; inspect or change it with `mugon-editor config` / `mugon-editor config --set-library <dir>`.

1. **Import** Synty packs. Download the Unity packages (`*.unitypackage`) from your Synty account and import them:

   ```
   mugon-editor import path	o\SIMPLE_Racer_Unity_2020_3_v1_5.unitypackage
   mugon-editor import path	oolder\with\packages\       # every .unitypackage in the folder
   ```
   Packs that are already in the library are **skipped** (add `--force` to re-import them), so you can point it at the same
   download folder again after adding new packages. A re-imported pack is rebuilt from scratch; your tag/category edits are kept.
   ~9 packs take ~3 minutes. Each pack's `thumbnails/` folder gets a `.gitignore` for `*.png`, so a library kept in git does not
   grow with previews (`mugon-editor thumbs` regenerates them).

2. **Render thumbnails** (2D previews; headless, ~15 ms per asset):

   ```
   mugon-editor thumbs                     # all packs; add --pack Simple_Racer, --filter barrel, --force
   ```

3. **Browse**:

   ```
   mugon-editor serve                      # then open http://127.0.0.1:7878
   ```
   Search, filter by pack / category / tag, click an asset for the 3D view (texture variants, animations, wireframe, bounds).
   You can add/remove tags and change categories there; edits are stored in `<library>/overrides.json`.
   "Open in editor" starts the editor with that asset armed.

4. **Let an agent use it** (MCP over stdio). Example Claude Code config:

   ```json
   { "mcpServers": { "mugon-editor": { "command": "mugon-editor", "args": ["--library", "D:\\asset-library", "mcp"] } } }
   ```
   Tools: `get_library_summary`, `list_packs`, `list_categories`, `list_tags`, `search_assets`, `get_asset` (absolute glTF path,
   texture, variants), `find_similar`.

5. **Edit scenes**:

   ```
   mugon-editor [--scene scenes/my_level.mugon-scene.json] [--game-dir ../game]   # uses the configured library
   ```
   Without a subcommand `mugon-editor` prints its help and opens the editor. The game folder (`--game-dir`, default: the current
   directory) configures components and folders through its `mugon.toml` (see below).
   Press `F1` or use the **Shortcuts** hint in the viewport's bottom-left corner (or Help → Shortcuts) for an expandable list of all keys and mouse controls.

   | Action | How |
   |---|---|
   | Fly camera | hold right mouse button + `W A S D`, `Q`/`E` down/up, `Shift` faster, wheel while flying = speed |
   | Pan / dolly | middle mouse drag / mouse wheel |
   | Focus selection | `F` |
   | Place a model | drag a thumbnail from the left panel into the viewport (ground or on other models); `Esc` while dragging cancels |
   | Select | click a model; click empty space to deselect; drag a rectangle to select everything it touches; `Shift`/`Ctrl` adds / toggles |
   | Copy / paste | `Ctrl+C` / `Ctrl+V` (pasted under the cursor) |
   | Nudge | arrow keys (camera-relative on the ground plane), `PageUp`/`PageDown` for height; 0.1 m or the grid size, `Shift` = x10, hold to repeat |
   | Tools | `W` move, `E` rotate, `R` scale, or the buttons at the right edge of the viewport. Drag the gizmo handles |
   | Snapping | toolbar: **Grid** (distance), **Snap to neighbours** (aligns/touches faces of nearby models), **rot** step. Snapping is automatic while enabled |
   | Undo / redo | `Ctrl+Z` / `Ctrl+Y` |
   | Duplicate / delete | `Ctrl+D` / `Del` |
   | Save | `Ctrl+S` or File menu (`*.mugon-scene.json`, dialogs start in the game's scene folder). **Export on save** (File menu, stored in the scene) also writes the BSN file and copies the used assets into the game |
   | Markers | **Markers: Box / Sphere / Cylinder** above the asset grid (or Edit → Add marker): translucent volume placeholders. Place them like models, size them with the Scale tool, add components to mark spaces for game logic |
   | Favorites | click the star in a thumbnail's corner; favorites appear in a collapsible **Favorites** section above the asset grid (narrowed by the search box only) and are saved in the scene file |
   | Components | select an entity, "Add component" in the inspector. Definitions come from `[[editor.component]]` in the game's `mugon.toml` |
   | Parenting | "Parent" dropdown in the inspector |

   The editor reads the `[editor]` table of the `mugon.toml` in the game folder (see `config/mugon.example.toml`):

   ```toml
   [editor.scenes]
   json-path = "scenes"                  # scenes (*.mugon-scene.json); default shown
   bsn-path = "src/scenes"               # exported BSN (<scene>.rs); default shown
   bsn-use = ["crate::components::*"]    # `use` lines in the exported BSN; default none

   [editor.assets]
   path = "assets"                       # "Export on save" copies the used assets to <path>/<pack>/...; default shown

   [[editor.component]]
   name = "Health"
   fields = [{ name = "max", type = "f32", default = 100.0 }]
   ```
   Paths are relative to the game folder (absolute paths work too). Field types: `bool, i32, f32, string, vec2, vec3, color, enum, asset`.

6. **Export to the game** (BSN):

   Turn on **File → Export on save** to write the BSN and copy the used assets on every save, use **File → Export BSN…** for a
   one-off export, or from the CLI (run in the game folder or pass `--game-dir`):

   ```
   mugon-editor export scenes/my_level.mugon-scene.json            # BSN into bsn-path + assets into the assets folder
   mugon-editor export scenes/my_level.mugon-scene.json --out src/level.rs --fn-name level_one   # BSN only
   ```
   * The BSN file contains `pub fn level_one() -> impl SceneList { bsn_list![ ... ] }`. Spawn it with the generated `spawn(world)` (a thin wrapper
     around `world.spawn_scene_list(level_one())`; use `commands.queue_spawn_scene_list` to spawn from systems). Every entity is exported with an explicit
     `Transform` and `Visibility` (also the identity), so transform/visibility propagation and avian's collider scale work for the whole hierarchy.
   * The asset copy puts only the models and textures the scene uses into `<assets>/<Pack>/...`, so the web build ships
     just what it needs. Models are loaded at runtime by asset path; the scene description is compiled into the game.
   * Markers export as `Transform` (scale = size) plus `Marker { shape: MarkerShape::Box }`; `Marker`/`MarkerShape` come from `mugon_sdk_bevy::scene` (feature `scene`), which exported scenes import themselves, so the game needs `mugon-sdk-bevy`.
     The same module has `markers::<C>(world)` (every entity with component `C`, with world pose, surface point and a point-in-volume test), `world_transform` (world pose right after spawning, before transform propagation)
     and, with feature `avian`, `StaticMeshColliderPlugin`/`StaticMeshCollider` (static trimesh colliders for the models of tagged entities). Any components you added to a marker are exported too.
   * Your game must define the component types used in the scene (matching `[[editor.component]]`) and make them
     `Default + Clone` (or `FromTemplate`). Bevy 0.19 has no `.bsn` file loader yet, which is why the export is Rust source.

## Command reference

| Command | Purpose |
|---|---|
| `mugon-editor import <pkg\|dir>... [--force]` | import `.unitypackage` files, skipping packs already imported (needs FBX2glTF; found next to the executable, or `--fbx2gltf` / `MUGON_EDITOR_FBX2GLTF`) |
| `mugon-editor config [--set-library <dir>]` | show / change the saved library folder |
| `mugon-editor [--scene file] [--asset id] [--game-dir dir]` | no subcommand: print help and start the editor with the configured library |
| `mugon-editor list` | packs and asset counts |
| `mugon-editor thumbs [args]` | render thumbnails (`--pack`, `--filter`, `--force`, `--size`, `--limit`) |
| `mugon-editor serve [--addr 127.0.0.1:7878] [--editor <exe>]` | browser UI + REST API |
| `mugon-editor mcp` | MCP server on stdio |
| `mugon-editor export <scene> [--out <file.rs>]` | BSN export and asset copy into the game folders |

Global options: `--library <dir>` / `MUGON_EDITOR_LIBRARY` (default: `[editor] library` in `~/.mugon/config.toml`), `--game-dir <dir>` (default: the current directory).

## Repository layout

```
crates/            Rust workspace (core, import, server (+ ui/), mcp, scene, thumbs, editor, cli, bsn-check)
config/            example mugon.toml [editor] table
docs/              ARCHITECTURE.md, DECISIONS.md, DEVELOPMENT.md
scripts/           packaging and install scripts used by CI
.gitlab-ci.yml     test + build + release pipeline
PLAN.md            requirements, roadmap and current status
AGENTS.md          orientation for AI coding sessions
```

Design, decisions, known gaps and how to develop: start with [`PLAN.md`](PLAN.md) and [`docs/`](docs/).
