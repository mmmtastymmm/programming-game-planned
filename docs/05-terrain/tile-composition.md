# Tile Composition & Corridors

*Part of [05-terrain](../05-terrain.md).*

## Tile Composition (the layer model)

*Decided 2026-07-26.* Every tile is two independent axes: **what the viewer knows** (the three fog states — in view / discovered-but-stale memory / undiscovered; see Fog of War) and **what is physically there**. The physical side is a strict either/or, plus one orthogonal occupancy fact (3):

1. **An unwalkable building (exclusive).** A building that can't be walked on — the Barricade today — owns its tile outright: it shares with *nothing*. No terrain surface renders under it, no overlay, no paint. Placing one clears the tile's designations (they're signage about traffic, and a blocked tile has none); demolishing it leaves bare ground to re-mark. The sim already models this shape — a blocking building replaces the tile kind (`Plains → Barricade`).
2. **Ground (a three-slot stack).** Anything walkable carries:
   - **Surface** — the terrain type (the table above). *Walkable buildings* provide a surface the same way: a Bridge's deck stands in for the Water under it, and future walkable structures (an elevator, who knows) would slot in identically. The surface sets move cost and terrain effects.
   - **Overlay** (at most one) — traffic rules: the Arrow. Composes with any surface, bridges included (the roundabout idiom depends on it).
   - **Paint** (at most one) — a color, and the new **routing layer**: the pathing builtins take optional per-call **`only=` / `avoid=`** paint arguments (Q95 — a color constant or a list of them; signatures in [01-language.md](../01-language.md)), and the pathfinder plans only across colors the call allows. Programs never name tiles (Q80) — paint is spatial vocabulary without coordinates: paint the haul road green, write `move_to(depot, only=green)`, and route policy is a brush stroke plus one keyword instead of waypoint code.

3. **A solid occupant** (the P27 ruling, 2026-08-01). Every unwalkable structure that isn't the Barricade — Depot, printers, the Upgrade Station, nests, both Pump tiles — is an **entity standing on the ground stack**, never a tile kind. Solidity derives from the structure registry (`structure_at` feeds pathing's blocked set, the Q120 displacement BFS's exclusions, and spawn guards — the shape the sim already implements), and the stack persists beneath, **inert, not erased**: the Pump's intake keeps the Water it pumps, paint and overlays sleep under a Depot and wake the moment it is demolished. Demolition removes the occupant and nothing else — what a structure leaves behind is exactly what it stood on. The Barricade alone keeps class 1's tile-kind exclusivity (Q99, shipped).

Paint semantics (v1, Q95/Q96): the constraint binds route *choice*, not physics — for that one route search the forbidden colors are impassable exactly like water (unreachable destination = the standard no-path fault), but a bump-shove, an Ice slide, or an engine walk that lands a bot on one is legal and faults nothing (a hard wall of paint would be a free Barricade). Constraints are **per-call, never per-bot**: no persistent binding, no new sim state beyond the paint map, and code that passes no colors is paint-blind. **Paint sabotage is legal play, destination tiles included** (Q109): painting the ground under a rival's Depot in a color their haulers `avoid=` strands that color — the same weapon as demolishing a bridge behind a raiding party, with the same counters (repaint it, kill the painter who had to stand in your territory, or stop passing `avoid=` on a critical route). No final-step exemption and no paint cooldown: the router rule stays clean, and the bite was mostly `fault_damage`, since a crash-looping bot dies on one clock whatever caused the loop. **`unpainted` is a color** — a pre-bound constant, so the args stay literal (`only=green` is a strict road; `only=[green, unpainted]` admits bare ground; `avoid=unpainted` pins a bot to painted surface) and the paint tool's eraser is just painting `unpainted`. The args ship free with their verbs. Ownership is ruled too (Q97): **paint is global** — one physical layer, no per-faction copies; a bot reads whatever is actually on the ground. Painting is **labor, not a click** (the blueprint flow: the player designates, a bot applies), so repainting a rival's road is legal play with a body on the line — seen, fought, and repainted back.

## Narrow Corridors & Traffic Tools

Bots are solid and bump-freezes are expensive ([02-agents.md](../02-agents.md)), so a one-tile corridor is a real engineering problem: two bots meeting head-on inside one **deadlock** — mutual bump, freeze, re-plan (no route), bump again, forever. **The engine will not solve this for you.** Traffic is player code; the toolkit is a ladder:

| Tier | Tool | The fix it enables |
|---|---|---|
| 0 | `wait(n)` + `rng(n)` | `wait(rng(20))` desynchronizes identical programs — stagger departures, time-slice the corridor |
| 2 | sensors + `if` | Check before committing (`path_blocked()` — real as of Q79 — plus occupancy peeks) |
| 6–7 | enums + **channels** | The real answer: a one-receiver channel token is a **mutex with a lease** (round 4) — hold the token to enter, `send` it back on exit, and the gatekeeper's `receive` timeout is the lease: a holder that crashes (handler restarts clear its token variable) or wrecks simply times out, and the gatekeeper's own fault-restart mints a fresh token. Lost locks recover; a wrecked holder still *physically* plugs a one-tile corridor — that's the drama, not a bug. (Give the gatekeeper an `on error:` window, or each lease expiry chips it — timeouts are ordinary faults) |
| terraform | bridges + **arrow overlays** / the Clear blueprint | Widen the corridor — or arrow two crossings in opposite directions: a deadlock-free roundabout, no mutex required ([Terraforming](#terraforming-build--deconstruct)) |

Design intent: corridor congestion is the first *systems* problem a colony hits — visible (frozen bots stare at each other), diagnosable (crash-free, just slow), and solvable at every tier with the tools of that tier. A deadlocked corridor is not a bug; it's the tutorial for channels.

