*Part of [05-terrain](../05-terrain.md).*

# Corruption is the thematic centerpiece

Corruption attacks the player's core resource — computation:

- Every Pyrite operation costs +1 cycle inside it (via its biome overlay) → a 10-line smart program crawls; a 3-line dumb one barely notices. **Terrain that inverts the "better code wins" rule locally.**
- Channel traffic (`send`/`receive`/`broadcast`) is jammed → coordinated squads decohere, blocked receivers inside never wake; bots must be individually competent to fight there. **Cloud telemetry is exempt** (Q76): `upload_log()`, crash dumps, and black-box banking always get through — the jam blocks bot-to-bot radio, not the engine's uplink. The logs always go home, even from inside the static.
- Crystal (needed for Chips → better CPUs) spawns near Corruption → the resource that buys computation lives where computation is worst. Deliberate loop.
- Scouting-track L3 veterans are immune to the cycle tax ([02-agents.md](../02-agents.md)) — XP as terrain key: the only bots whose *code* runs clean in there.

## Corruption is alive (dynamics)

- **Corruption radiates from sources** — Blight Cores seeded by mapgen, and nests that spread it (the Devil, [04-enemies.md](../04-enemies.md)). Tiles corrupt outward slowly toward each source's radius.
- **Cleansing works, and doesn't last** (the Cleanse blueprint). Cleansed tiles re-corrupt while their source survives. Treating symptoms buys time (a corridor to the Crystal, a breathing spell for a claim); **rooting out the source is the only cure** — and sources sit deep in the zone, where your code runs worst.
- Left alone, Corruption **comes back and keeps coming**: an untended frontier slowly re-corrupts, pressuring claims, channels, and supply lines. It's the PvE antagonist that never idles.

