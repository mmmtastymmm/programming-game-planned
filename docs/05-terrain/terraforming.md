*Part of [05-terrain](../05-terrain.md).*

# Terraforming (build & deconstruct)

The map is editable — both directions. **Designation is the player's; labor is code**: the player places a **blueprint** on a target tile (a UI act — one lockstep Command, charged on placement), and bots service it with `move_to(closest(blueprint).expect())` + `build()` (1 progress/tick, adjacent, earns Building XP; several bots stack). Programs never name tiles — Pyrite has no position literals, and doesn't need them. Terraform **blueprint types** (Q80 — these are *not* functions: placing one is a Command, the Cache find unlocks the ability to place them, and bots service them with `build()`; unlocked after `build`/`repair`, [06-progression.md](../06-progression.md)):

| Blueprint | Effect | Cost |
|---|---|---|
| **Clear** | Rubble → Plains; yields a little **Stone** | build time |
| **Bridge** | Water → Bridge (ground-passable) | Stone + build time |
| **Barricade** | Plains → Barricade (blocks movement **and vision** — it's tall). **Has HP; attackable** (Q99): a wall is a target, so a siege is a real option beside the Demolish crew | Stone + build time |
| **Demolish** | remove Bridge / Barricade | build time |
| **Cleanse** | Corruption → Plains (see Corruption dynamics — it grows back) | build time, slow |
| **Road** | Plains / Rubble → Road (half plains move cost) | Stone + build time |

Deconstruction is symmetric and adversarial: enemies can `demolish` **your** bridge — behind your raiding party. Chokepoints stop being facts of the map and become claims you defend.

Beyond buildings, two **designation layers** sit on top of any tile:

- **Overlays** — traffic rules, instant signage (no build labor). An **Arrow** makes its tile one-way (enter and leave only along the arrow; small cost; clearable). Arrows on a bridge = a directional crossing; opposing arrowed bridges = a deadlock-free roundabout; arrows on plain ground = dedicated lanes.
- **Paint** — tile color, promoted (2026-07-26) from cosmetic to the **routing layer**: the pathing calls take `only=`/`avoid=` color arguments (see Tile Composition, below). Painting follows the **blueprint flow** (Q97): the player draws the colors — a designation Command — and a bot must travel there to apply them (quick per tile, no material cost; labor and exposure are the price). **The layer is global**: one physical coat per tile, no faction ownership — anyone's bot can repaint anyone's ground, so paint sabotage is legal, visible, and fightable (the demolish-their-bridge precedent). Erasing is painting `unpainted`. Paint still doubles as zoning and notes-to-self, and the `paint_at()` sensor hook stands.

