*Part of [04-enemies](../04-enemies.md).*

# Nest Allegiance — the Major Arcana (0–21)

Every Nest has an **Allegiance**, numbered 0–21 after the tarot Major Arcana. **The number is the difficulty axis** — higher allegiance means better-written programs, higher construct tiers, and nastier tricks. The arcanum is the nest's *personality*: what it prints, how it fights, and above all **how it treats code** — whether its programs are static, mutated between prints, or actively researched.

All of this is first-pass flavor to tune; the mechanical skeleton (allegiance number → code-behavior flags) is the part to build.

| # | Arcanum | Nest identity | Code behavior |
|---|---|---|---|
| 0 | The Fool | Tier-0 straight-line bots that wander into things and fault constantly. Poses no real threat — the tutorial nest. | Static; ships with bugs *on purpose* (reading its crash-loops is the first lesson) |
| 1 | The Magician | Loves to create: every print carries a small mutation — no two of its Drones run identical code. | **Procedural mutation**, minor (tweaked constants, reordered lines) |
| 2 | The High Priestess | Silent intelligence: stealth scouts that shadow your bots and **collect your Black Boxes** before you do. | Static, sensor-heavy; steals intel rather than dealing damage |
| 3 | The Empress | Fertility: double print rate, Harvester floods, buds **satellite nests**. Wins by growth, not combat. | Static economy scripts, excellently tuned |
| 4 | The Emperor | Order: officer bots broadcast commands to ranks. Kill the officer and the formation decoheres to Tier-1 behavior. | Static, messaging-heavy hierarchy |
| 5 | The Hierophant | The teacher: deploys textbook-perfect demos of constructs you haven't unlocked — and **converts**: attempts to `hijack()` your disabled wrecks into its flock. | Static exemplars; hijack-capable |
| 6 | The Lovers | Bonded pairs: units fight in twos; when one dies, its partner hot-swaps to an avenger program. | Static, signal-linked pairs |
| 7 | The Chariot | Speed: fast raid swarms on straight-line assault vectors, terrain-ignorant pathing (exploitable at chokes). | Static rush scripts |
| 8 | Strength | Few, heavy, patient: high-HP hunters that **target your highest-XP bots** first. | Static; priority logic reads XP decals |
| 9 | The Hermit | Lone elites far from any nest; the nest itself is hidden and must be scouted to be ended. | Static, self-sufficient (long programs, big CPU) |
| 10 | Wheel of Fortune | Chance: patrol routes, targets, even cycle budgets rolled from seeded RNG streams. Unreadable by pattern, only by code. | **Procedurally randomized parameters** per print |
| 11 | Justice | The ledger: retaliates in proportion to each player's aggression — tit-for-tat tracked per player (multiplayer-aware). | Static but **stateful**: grudge counters in colony memory |
| 12 | The Hanged Man | Sacrifice: scuttle-bombers that weaponize `abort()` — deliberate scuttles that plant ticking wrecks on your doorstep: clear them in time or eat the countdown explosion (each wreck rides its *own* countdown — blasts never chain, Q76). | Static, scuttle-centric |
| 13 | Death | The recycler: **salvages every wreck on the field** — yours, other Ferals', its own — to fuel printing. Starves your salvage economy and eats your battlefields. | Static; salvage-centric |
| 14 | Temperance | Balance: reads your army composition and prints proportional counters. The first nest that **researches** — its tech keeps pace with yours. | **Researches**; adaptive mix |
| 15 | The Devil | Corruption: spreads Corruption biome tiles outward and **hijacks your bots** — reprogrammed veterans fight for it, XP intact. | Hijack-capable; terrain-altering |
| 16 | The Tower | Ruin: ignores your bots entirely; sudden all-in lightning raids on structures — Fabricators and Archives first. | Static siege scripts, long dormancy between strikes |
| 17 | The Star | Guidance: relay beacons that extend **other nests'** broadcast range and repair their units. Kill the support first. | Static, cross-nest cooperative |
| 18 | The Moon | Illusion: decoy units running deliberately misleading (but real) programs; forges **fake Black Boxes** with lying logs. Trust nothing on this part of the map — even what you've decrypted was *written to be decrypted*. | **Procedural counter-intel**; dishonest by design |
| 19 | The Sun | Clarity: no tricks — simply the best straightforward combat programs in the game, surging on full Energy. Honest and terrifying. | Static, peak-quality authored code |
| 20 | Judgement | Resurrection: reboots its dead **with XP intact** — its veterans accumulate all match. Leave no wrecks, or face them again, stronger. | Static; XP-preserving reprints |
| 21 | The World | Completion: rotates through the behaviors of every lower arcanum and uses the full construct set. The endgame nest. | **Researches + procedurally mutates**; everything |

## What Allegiance controls (the mechanical flags)

- **Program quality**: which construct tiers ([01-language.md](../01-language.md)) and function blocks its scripts use. Roughly: arcana 0–4 preview Tiers 0–2, 5–13 preview Tiers 3–5, 14+ use things players are still saving Data for.
- **Code modification** (your Magician instinct, generalized): `static` (most) / `mutates-per-print` (1, 10, 18, 21) / `researches` (14, 21 — these escalate their own tree over the match, answering "should nests research?": *some do, by arcanum*).
- **Mutation style**: authored variants vs. procedural — set **per nest type and biome**. A Magician nest in Corruption mutates handlers; one in a Loop Desert unrolls loops. Biome cost overlays ([05-terrain.md](../05-terrain.md)) shape what mutations are *viable*, so the same arcanum plays differently across the map.
- **Map placement**: allegiance scales with distance from player starts — 0–4 near start zones, 5–13 midfield, 14–21 deep field. The **maximum arcanum on a map is a match option** (available on any server type, PvP included) — raising it doesn't make the neighborhood meaner, it makes the *frontier* deeper. Allegiance is geography as much as clock.
