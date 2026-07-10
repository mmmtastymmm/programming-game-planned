# Terrain

Rule: **every terrain type must change what a good program looks like.** If a tile type doesn't alter movement, sensing, resources, or computation, it doesn't ship. The map is a tile grid (fits the deterministic sim and integer math — see [08-multiplayer.md](08-multiplayer.md)).

## Tile Types

| Terrain | Move cost | Effects | The program it demands |
|---|---|---|---|
| **Plains** | 1× | none | baseline |
| **Rubble** | 2× | — | Pathing tradeoffs: `move_to` auto-paths, but route *choice* (waypoints) is player code |
| **Ore Vein** | 1× | minable Ore node | mining loops |
| **Crystal Field** | 1× | minable Crystal; usually spawns near Corruption | risk-managed harvesting (`if can_see_feral(): flee`) |
| **Geothermal Vent** | 1× | only tile allowing Geothermal Tap | expansion targets worth fighting over |
| **Mud** | 3×, and loaded bots 4× | — | haulers should route *around*; naive `move_to(depot)` straight-lines through it |
| **Water** | impassable (ground) | blocks ground bots; conducts sensor pings farther | natural walls; chokepoint defense |
| **High Ground** | 1×, enter only via Ramp tiles | +2 sensor range, +25% ranged damage down | king-of-the-hill fights; scout perches |
| **Corruption** | 1× | bots suffer **+1 cycle cost on every operation**; no channel traffic (`send`/`receive`) in/out; Ferals spawn here | *the signature tile*: your code literally runs worse here — simple short programs outperform clever long ones inside Corruption |

## Biome cost overlays

The Pyrite cycle-cost table is data with **per-biome overlays** ([01-language.md](01-language.md), [07-architecture.md](07-architecture.md)): any map or biome can override any operation's cost, including the fault penalty. This is the general mechanism for terrain that stresses *program designs* rather than stats. Shipped and speculative examples:

| Biome overlay | Override | Design it punishes / rewards |
|---|---|---|
| **Corruption** (shipped first) | every op +1 | punishes long clever programs |
| Static Wastes | `send` ×3 | punishes swarm coordination |
| Loop Desert | loop iteration ×3 | punishes iteration-heavy code, rewards unrolled/flat code |
| Overclock Field | all ops −1 (min 1), crash-dump cost ×2, grace window halved | rewards bold code, makes bugs expensive |

Map authors pick overlays per biome; the editor shows *effective* per-line costs for the tile the selected bot stands on.

## Terraforming (build & deconstruct)

The map is editable — both directions, by anyone with the tools (`terraform` function blocks, unlocked after `build`/`repair`, [06-progression.md](06-progression.md)):

| Action | Effect | Cost |
|---|---|---|
| `clear(tile)` | Rubble → Plains | build time |
| `bridge(tile)` | Water → Bridge (ground-passable) | Metal + build time |
| `barricade(tile)` | Plains → Barricade (blocks movement **and vision** — it's tall; has HP, attackable) | Metal + build time |
| `demolish(tile)` | remove Bridge / Barricade | build time |
| `cleanse(tile)` | Corruption → Plains (see Corruption dynamics — it grows back) | build time, slow |

Deconstruction is symmetric and adversarial: enemies can `demolish` **your** bridge — behind your raiding party. Chokepoints stop being facts of the map and become claims you defend.

## Narrow Corridors & Traffic Tools

Bots are solid and bump-freezes are expensive ([02-agents.md](02-agents.md)), so a one-tile corridor is a real engineering problem: two bots meeting head-on inside one **deadlock** — mutual bump, freeze, re-plan (no route), bump again, forever. **The engine will not solve this for you.** Traffic is player code; the toolkit is a ladder:

| Tier | Tool | The fix it enables |
|---|---|---|
| 0 | `wait(n)` (function block, cost 1 + n idle ticks) | Stagger departures; crude time-slicing of a shared corridor |
| 2 | sensors + `if` | Check before committing (candidate blocks: `path_blocked()`, occupancy peeks) |
| 6–7 | enums + **channels** | The real answer: a one-receiver channel token **is a mutex** — hold the token to enter the corridor, `send` it back on exit; gatekeeper bots at each mouth |
| terraform | `bridge()` / `clear()` | Widen the corridor: turn the traffic problem into infrastructure ([Terraforming](#terraforming-build--deconstruct)) |

Design intent: corridor congestion is the first *systems* problem a colony hits — visible (frozen bots stare at each other), diagnosable (crash-free, just slow), and solvable at every tier with the tools of that tier. A deadlocked corridor is not a bug; it's the tutorial for channels.

## Fog of War (decided: eyes only)

**Vision is the live union of every friendly bot's and structure's sensor range. Nothing else.**

- No permanent "explored" reveal. The UI keeps a **greyed terrain snapshot** of last-seen tiles (you remember the shape of the land), but live state — units, resources remaining, nest status — exists only where something of yours is looking *right now*.
- Scouting is therefore **infrastructure, not an event**: standing watch is a job bots do (and earn Scouting XP for, [02-agents.md](02-agents.md)). A cheap Sentry Post structure exists for fixed sightlines ([03-resources.md](03-resources.md)).
- **Tall things block vision.** Sensors are line-of-sight: Barricades and cliff faces cut sightlines. High Ground sees *over* Barricades — height beats walls, which is half of why perches matter. Corollary: walling your base in also blinds it; pair walls with Sentry Posts or high ground.
- Terrain hooks apply: High Ground +2 sensor range, Water conducts sensor pings farther, Scouting-track veterans see farther.
- **Ally vision sharing is a grant**, like channels ([01-language.md](01-language.md)) — allied colonies choose to pool eyes; it isn't automatic.

## Corruption is the thematic centerpiece

Corruption attacks the player's core resource — computation:

- Every Pyrite operation costs +1 cycle inside it (via its biome overlay) → a 10-line smart program crawls; a 3-line dumb one barely notices. **Terrain that inverts the "better code wins" rule locally.**
- Channel traffic (`send`/`receive`) is jammed → coordinated squads decohere, blocked receivers inside never wake; bots must be individually competent to fight there.
- Crystal (needed for Chips → better CPUs) spawns near Corruption → the resource that buys computation lives where computation is worst. Deliberate loop.
- Scouting-track L3 veterans are immune to the cycle tax ([02-agents.md](02-agents.md)) — XP as terrain key: the only bots whose *code* runs clean in there.

### Corruption is alive (dynamics)

- **Corruption radiates from sources** — Blight Cores seeded by mapgen, and nests that spread it (the Devil, [04-enemies.md](04-enemies.md)). Tiles corrupt outward slowly toward each source's radius.
- **`cleanse()` works, and doesn't last.** Cleansed tiles re-corrupt while their source survives. Treating symptoms buys time (a corridor to the Crystal, a breathing spell for a claim); **rooting out the source is the only cure** — and sources sit deep in the zone, where your code runs worst.
- Left alone, Corruption **comes back and keeps coming**: an untended frontier slowly re-corrupts, pressuring claims, channels, and supply lines. It's the PvE antagonist that never idles.

## Map Composition Guidelines

```mermaid
flowchart TD
    subgraph MapRing["Typical map, center-out"]
        S[Start zones:<br/>Plains + small Ore + 1 Vent] --> M[Midfield:<br/>Rubble, Mud, larger Ore veins,<br/>contested Vents]
        M --> C[Deep field:<br/>Crystal + Corruption + Feral Nests,<br/>High Ground overlooks]
    end
```

- **Start zones are safe and legible** — a Tier-0 program works there. Difficulty is geographic.
- **Template Caches ring each start zone** ([06-progression.md](06-progression.md)): basic ones close, advanced ones toward the midfield. They're non-consumable study sites — everyone can learn from them, so the deep ones are worth *holding*, not racing. The opening toolkit sweep is the first thing eyes-only fog makes interesting.
- **Every expansion is a tradeoff**: more Ore = longer haul routes; Crystal = Corruption exposure; Vents = contested.
- **Chokepoints from Water/High Ground** give defensive programs something to anchor on (`guard(ramp_tile)`).
- PvP maps are **mirror-symmetric**; co-op maps are asymmetric with a shared frontier.

## Terrain × Systems Matrix

| System | Terrain interaction |
|---|---|
| Language ([01](01-language.md)) | Corruption cycle tax; move costs multiply `move_to` action time |
| Agents ([02](02-agents.md)) | Scout perk vs Corruption; loaded-hauler mud penalty |
| Resources ([03](03-resources.md)) | All raw resources are terrain-placed; Vents gate free energy |
| Enemies ([04](04-enemies.md)) | Nests anchor in Corruption; Feral patrol routes follow terrain graph |
| Multiplayer ([08](08-multiplayer.md)) | Tile grid + integer move costs keep pathing deterministic |

## Decided

- **Terraforming is in scope** — build (bridges, barricades) and deconstruct (clear, demolish, cleanse), symmetric and adversarial (see Terraforming).
- **Fog of war is eyes-only** — live union of friendly bot + structure sensors; greyed terrain memory, no persistent live intel (see Fog of War).
- **Tall things block vision** — sensors are line-of-sight; Barricades are true walls; High Ground sees over them.
- **Corruption is dynamic** — radiates from sources, re-corrupts cleansed ground until the source is destroyed (see Corruption dynamics).

## Open Questions

- Corruption spread/re-corruption rates, source radii, and cleanse speed — pure tuning, needs the prototype.
