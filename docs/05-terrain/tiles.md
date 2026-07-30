# Tile Types & Biome Overlays

*Part of [05-terrain](../05-terrain.md).*

## Tile Types

| Terrain | Move cost | Effects | The program it demands |
|---|---|---|---|
| **Plains** | 1× | none | baseline |
| **Rubble** | 2× | — | Pathing tradeoffs: `move_to` auto-paths, but route *choice* (waypoints) is player code |
| **Ore Vein** | 1× | minable mineral node — Iron, Coal, Copper, Tin, Silver, or Gold variant ([03-resources.md](../03-resources.md)); deeper/rarer kinds sit farther from start zones | mining loops |
| **Grove** | 1× | harvestable Wood; **regenerates** | renewable-but-thin logging loops |
| **Outcrop** | 1× | harvestable Stone node — plentiful, near everywhere ([03-resources.md](../03-resources.md)) | fortification supply lines: walls are hauled |
| **Sand Flat** | 1× | harvestable Sand — shoreline flats and dune fringes ([03-resources.md](../03-resources.md)); deep **Dunes** (below) make *interior* sand risky to work: a harvesting bot is standing still, and the sinking clock ticks | glassworks supply; another reason coasts are contested |
| **Crystal Field** | 1× | minable Crystal; usually spawns near Corruption | risk-managed harvesting (`if exists(enemy): move_to(closest(repair_bay).expect())`) |
| **Geothermal Vent** | 1× | only tile allowing Geothermal Tap | expansion targets worth fighting over |
| **Mud** | 3×, and loaded bots 4× | — | haulers should route *around*; naive `move_to(depot)` straight-lines through it |
| **Water** | impassable (ground) | blocks ground bots; shoreline tiles accept a **Pump** (the Water resource, [03-resources.md](../03-resources.md)) | natural walls; chokepoint defense — and now a resource worth holding |
| **High Ground** | 1×, enter only via Ramp tiles | +2 sensor range, +25% ranged damage down | king-of-the-hill fights; scout perches |
| **Corruption** | 1× | bots suffer **+1 cycle cost on every operation**; no channel traffic (`send`/`receive`) in/out; Ferals spawn here | *the signature tile*: your code literally runs worse here — simple short programs outperform clever long ones inside Corruption |
| **Dunes** | 2× | **idling sinks** (Q35): stand still longer than N ticks and the exit cost escalates | sand punishes loitering — `wait(n)` staging and rally points are unsafe here; keep moving |
| **Mountain** | **edge-cost** (Q36): climbing on is expensive, descending moderate, ridge-to-ridge 1× | summit tiles carry High Ground's +2 sensor state — the soft-slope sibling of ramp-gated High Ground | ranges are highways with costly on-ramps: route *along* them, budget the climb |
| **Ice** | 1×/tile, **uncontrolled** | entering continues the move in the same direction until non-ice — a deterministic slide (Q37); an arrow overlay mid-slide *redirects* it; sliding into an occupied tile is a normal bump (slider = rammer) and ends the slide — except engine walks (recall), which never bump the mover (Q73) | plan slide endpoints; mass-produces `on bump:` use |
| **Ford** | 4× | mapgen-placed shallow crossings — *specific* tiles, not all water (Q38); wading grants a **signature bonus** (the water masks you — see Fog of War) | the slow, sneaky back door; bridges stay the fast contested chokepoint |
| **Road** | ½× | terraformed (the Road blueprint, Stone — see Terraforming); the ½ exists because move costs store at ×2 scale (Q39, below) | logistics arteries worth paving — and worth raiding |
| **Scree** | 2× | **collapses to Rubble after N crossings** (per-tile counter, Q40 — the natural-bridge-HP precedent) | the shortcut wears out: optimal programs rotate routes |
| **Snow** | 1× | **mutes movement** (Q78): a bot on Snow makes no movement noise — undetectable by *hearing* regardless of signature; only **seeing** finds it | the silent-approach biome: attackers route assaults over snow without creeping; defenders need *eyes* on the snowline (Sentries, Lanterns, patrols) — ears are useless there |

Move costs are integers in `costs.ron` stored at **×2 scale** (Plains 2, Road 1, Rubble 4, Mud 6/8 …) so Road's half-plains cost exists (Q39) — the same fine-grained-units medicine as Q56, and a one-time migration that buys tuning granularity everywhere. Footprints and a `tracks_at()` sensor (Q40's second half) are **deferred post-v1** — per-tile trace state and a new sensor surface haven't earned their sim cost yet.

## Biome cost overlays

The Pyrite cycle-cost table is data with **per-biome overlays** ([01-language.md](../01-language.md), [07-architecture.md](../07-architecture.md)): any map or biome can override any operation's cost, including the fault penalty. This is the general mechanism for terrain that stresses *program designs* rather than stats. Shipped and speculative examples:

| Biome overlay | Override | Design it punishes / rewards |
|---|---|---|
| **Corruption** (shipped first) | every op +1 | punishes long clever programs |
| Static Wastes | `send` ×3 | punishes swarm coordination |
| Loop Desert | loop iteration ×3 | punishes iteration-heavy code, rewards unrolled/flat code |
| Overclock Field | all ops −1 (min 1), crash-dump cost ×2 | rewards bold code, makes bugs expensive |

Overlays live on **authored regions** (Q101, 2026-07-26) — arbitrary areas the map defines, not tile kinds — which is what lets a biome have any shape and makes regions the natural home for **boss biomes**: a punishing zone around a boss, unrelated to the ground beneath it. Corruption is the exception that proves the rule: its tax stays **tile-based**, because the creep spreads tile by tile and killing a Blight Core must leave taxed ground behind (a region-scoped tax would die with the core and strip Cleanse of its purpose). The editor shows *effective* per-line costs for the tile the selected bot stands on.

Effective cost resolves in **three layers**, machine outward:

    floor₁( region_rule( tile_rule( base + Σ per-bot deltas ) ) )

Per-bot deltas (quirks, perks) apply **first, so terrain amplifies them**: Dial-Up (`send` +1) inside Static Wastes (`send` ×3) pays `(3+1)×3 = 12`, not `9+1`. A loud radio is *worse* in a jamming field — quirks get more dramatic under hostile ground rather than merely additive. **Each layer defines one RULE per key** (a specific row beats a general one — Overclock's crash-dump row wins over its all-ops row), so stacking is never ambiguous; only the bot standing there varies the result. No overlay or quirk can push an op below **1 cycle** (the global floor), and **forced charges are taxable** (Overclock's doubled crash dump works as written): they are charged as debt, so no overlay can strand a bot by making dying expensive. `bank_cap` is a flat generous ceiling validated at load. The check must evaluate the **whole pipeline at its worst case** — `region(tile(base + the largest cost-raising per-bot delta any quirk or perk can contribute))` for every key — not the overlays alone: per-bot deltas apply *inside* the multipliers, so a quirked bot (Dial-Up, Telemetry Enabled) in a multiplier region would otherwise sit outside the certified invariant and could face an op it can never bank for. Quirks are data too, so the worst case is computable at load. That keeps Q75/Q82's "freeze-forever is impossible" guarantee as a checked invariant instead of per-tick arithmetic — for *every* bot, not just unquirked ones.

