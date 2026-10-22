# Orientation for AI coding sessions

Project: **Mugon Editor**, a Rust workspace (asset library + Bevy scene editor) for the Mugon web game platform.

Read in this order: `README.md` (usage) → `PLAN.md` (requirements, status, roadmap) → `docs/ARCHITECTURE.md` →
`docs/DECISIONS.md` (why things are the way they are, insights) → `docs/DEVELOPMENT.md` (build/test, what is verified).

Essentials:
* Bevy **0.19**; BSN has **no file format** in 0.19, the exporter generates Rust `bsn_list!` source.
* The asset library lives outside the repo (`--library` / `MUGON_EDITOR_LIBRARY`); the dev library was `E:/asitor-library`.
* Keep Bevy out of `mugon-editor-core`, `mugon-editor-import`, `mugon-editor-scene`, `mugon-editor-server`, `mugon-editor-mcp`.
* After exporter changes: `UPDATE_GOLDEN=1 cargo test -p mugon-editor-scene`, then `cargo test --workspace`.
* The editor's interactive behaviour (mouse picking, gizmo drag, snapping) has never been driven by a script; treat it as
  untested beyond `--demo --screenshot`.
* Update `PLAN.md` status and add ADR lines to `docs/DECISIONS.md` when you change something significant. Commit at sensible points.
