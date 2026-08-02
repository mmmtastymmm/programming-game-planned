*Part of [06-progression](../06-progression.md).*

# The Two Scopes

Progression runs on two **scopes**:

| Scope | What | Rationale |
|---|---|---|
| **Permanent (account)** | **Language constructs** — variables, loops, `def`, lists, handlers, messaging (branching ships at game start — Q117) | *Knowledge.* Once a player has learned to use variables, they have them — forever, in every future match. The constraint stops being "can I say it" and becomes "how effectively can I say it." |
| **Per-match** | **Function blocks** (found at Caches — [template-caches.md](template-caches.md)), **program colors** (Green at start; repair the ruined Red printer with Data; more via controlled nests, [04-enemies.md](../04-enemies.md)), **hardware** (Upgrade Station) | *Situation.* What your colony can *do* this game is earned this game. |

A construct is permanently unlocked the first time it's researched in any match (its Data cost is paid once, ever). Function blocks re-unlock every match.

**PvP gate: all constructs must be permanently unlocked before entering PvP.** Every PvP player has the full language; matches are symmetric races over functions, colors, and hardware, decided by code quality. (Co-op has no gate — mixed-knowledge groups are fine, and the shared program library lets veterans hand working code to newer players.)

The three per-match tracks in detail (requirements 3b/3c):

1. **Language constructs** — what syntax your colony's programs may use (colony-wide; permanent scope). Unlocked by researching with Data, **in any PvE play** — first research ever = yours forever.
2. **Function blocks** — what built-ins programs may call (colony-wide, per-match; some also need a tool grade on the bot — Q111's tool model). **Learned, not researched** — studied at Template Caches ([template-caches.md](template-caches.md)).
3. **Hardware** (not research — purchased per-bot at the Upgrade Station, priced by resource role) — cycles/tick, program length, stack depth.
