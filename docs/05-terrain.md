# Terrain

Rule: **every terrain type must change what a good program looks like.** If a tile type doesn't alter movement, sensing, resources, or computation, it doesn't ship. The map is a tile grid (fits the deterministic sim and integer math — see [08-multiplayer.md](08-multiplayer.md)).

## The parts

| File | Owns |
|---|---|
| [tiles.md](05-terrain/tiles.md) | The tile types and the per-biome cost overlays. |
| [terraforming.md](05-terrain/terraforming.md) | Building and deconstructing terrain. |
| [tile-composition.md](05-terrain/tile-composition.md) | The layer model (paint as the routing layer) and narrow-corridor traffic tools. |
| [fog-of-war.md](05-terrain/fog-of-war.md) | What a faction knows, and how seeing differs from hearing. |
| [corruption.md](05-terrain/corruption.md) | The thematic centerpiece and its spread. |
| [map-generation.md](05-terrain/map-generation.md) | Authoring guidelines and the procedural generator. |
| [decided.md](05-terrain/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **A tile that changes no program doesn't ship** — canonical in this file's
  opening rule, above. It is the filter every addition in these parts must pass.
- **A tile is layers, not a kind** (2026-07-26) — canonical in
  [tile-composition.md](05-terrain/tile-composition.md). Base terrain, paint, and
  contents compose; paint is the *routing* layer and carries no material cost;
  solid structures are **occupants on the stack**, never tile kinds — the
  Barricade is the sole exception (P27).
  Forbidden paint is impassable-like-water, which routes into the standard
  no-path fault rather than a special case (Q95–Q97).
- **Overlays attach to regions, not tile kinds — with Corruption the exception**
  — canonical in [tiles.md](05-terrain/tiles.md), which keeps Corruption's
  tile-based tax. That is why the pipeline has three layers
  (Q101): `floor₁( region_rule( tile_rule( base + Σ per-bot deltas ) ) )`.
- **Effective cost is bounded by `bank_cap`** — canonical in
  [01-language/execution-model.md](01-language/execution-model.md), verified at
  load against the worst case rather than per tick. Any new overlay here must keep that check passing —
  see [01-language.md](01-language.md).
- **Seen tiles are sim state** (Q94) — canonical in
  [decided.md](05-terrain/decided.md) — and so is the **known-structures
  memory** (P22): own structures and designations always known, foreign as
  last observed, `faction=own` query default, pooled by the ally vision
  grant. Neither is a rendering artifact, so fog belongs to the
  deterministic world and not the `game` crate.
- **All numbers here are tuning constants** bound for data files, never code —
  canonical in CLAUDE.md's doc conventions.

## Terrain × Systems Matrix

| System | Terrain interaction |
|---|---|
| Language ([01](01-language.md)) | Corruption cycle tax; move costs multiply `move_to` action time |
| Agents ([02](02-agents.md)) | Scout perk vs Corruption; loaded-hauler mud penalty |
| Resources ([03](03-resources.md)) | All raw resources are terrain-placed; Vents gate free energy |
| Enemies ([04](04-enemies.md)) | Nests anchor in Corruption; Feral patrol routes follow terrain graph |
| Multiplayer ([08](08-multiplayer.md)) | Tile grid + integer move costs keep pathing deterministic |
