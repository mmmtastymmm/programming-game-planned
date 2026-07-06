# Enemies — The Feral

The PvE faction: **Feral machines**, corrupted bots left over from whatever wrecked this world. The core conceit (requirement 5): **Ferals run real Pyrite programs on the same VM as player bots**, and players can *read those programs*.

## Why enemies run the player's VM

- **One interpreter, one truth.** No separate AI system to build or keep deterministic ([08-multiplayer.md](08-multiplayer.md)). Feral behavior is exactly as inspectable, steppable, and deterministic as player code.
- **Reading code is the counterplay.** A Feral's program is its stat block *and* its weakness. `if attacker_count > 2: flee_to_nest()` is an instruction to the player: bring three bots.
- **Enemies are the curriculum.** Early Feral programs are simple Tier-0/1 scripts that teach by example; late ones use constructs the player hasn't unlocked yet — a preview of their own future power.

## Inspection Mechanics

Ferals are **always fully open** — click and read, live. (Opposing *players'* programs are not: those are read only by salvaging a kill, [08-multiplayer.md](08-multiplayer.md). Ferals are the curriculum; players are the competition.)

| Method | What you see |
|---|---|
| Click any visible Feral | Its program source, read-only, with **live program counter** stepping |
| Analyze a Feral wreck (Artisan bot, `analyze()` function) | Full program copied to your **Codex** for offline study + grants **Data** ([03-resources.md](03-resources.md)) |
| Codex library | Every Feral program ever analyzed, diffable against variants |

Live program-counter view means a retreating player can literally watch the pursuer's code hit its `if distance_from_nest > 40: return_home()` line. Aha-moments by design.

## Feral Archetypes (initial set)

Each archetype = chassis + program. Programs shown are their *actual* shipped source.

### Drone (threat 1) — teaches Tier 0

```python
wander()
wander()
if can_see_bot():
    attack(nearest_bot())
```

Harmless in ones. Exists so the first program a player ever reads is trivially comprehensible.

### Stinger (threat 2) — teaches conditionals

```python
if health_low():
    flee_to(nest)
if can_see_bot():
    chase(nearest_bot())
    attack(nearest_bot())
wander()
```

Counterplay written in the code: hurt it and it *will* run — ambush the retreat path.

### Harvester (threat 2) — economic enemy

```python
target = nearest_ore()
move_to(target)
mine()
move_to(nest)
deposit()
```

Steals *your* map's ore and feeds its nest. Ignores bots entirely — a pure race pressure on the economy.

### Warden (threat 3) — teaches loops + messaging

```python
for spot in patrol_route:
    move_to(spot)
    if can_see_bot():
        broadcast("intruder", nearest_bot())
        attack(nearest_bot())
```

Patrols and *calls for help*. Counterplay: jam or bait the broadcast, or kill it inside one patrol leg.

### Nest (structure, threat scales)

Prints Ferals from harvested resources, exactly like a player Fabricator. Feral economy is real: starve the nest (kill Harvesters) and it prints less. Destroying a Nest yields a large Data bounty.

**Controlling nests is the territory game**: a defeated Nest can be **claimed** (an Artisan converts the site) instead of razed. Controlled nests gate how many **printers** — and therefore program colors — a colony may build, on a quadratic curve ([01-language.md](01-language.md), [03-resources.md](03-resources.md)). Razing pays Data now; claiming grows your program portfolio forever. Higher-arcana nests are worth the same slot credit but are far harder to take — pick your conquests.

## Nest Allegiance — the Major Arcana (0–21)

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
| 8 | Strength | Few, heavy, patient: Bulwark-class hunters that **target your highest-XP bots** first. | Static; priority logic reads XP decals |
| 9 | The Hermit | Lone elites far from any nest; the nest itself is hidden and must be scouted to be ended. | Static, self-sufficient (long programs, big CPU) |
| 10 | Wheel of Fortune | Chance: patrol routes, targets, even cycle budgets rolled from seeded RNG streams. Unreadable by pattern, only by code. | **Procedurally randomized parameters** per print |
| 11 | Justice | The ledger: retaliates in proportion to each player's aggression — tit-for-tat tracked per player (multiplayer-aware). | Static but **stateful**: grudge counters in colony memory |
| 12 | The Hanged Man | Sacrifice: scuttle-bombers that weaponize `become_disabled()` and `on death:` — its units *want* to die on your doorstep. | Static, handler-centric |
| 13 | Death | The recycler: **salvages every wreck on the field** — yours, other Ferals', its own — to fuel printing. Starves your salvage economy and eats your battlefields. | Static; salvage-centric |
| 14 | Temperance | Balance: reads your army composition and prints proportional counters. The first nest that **researches** — its tech keeps pace with yours. | **Researches**; adaptive mix |
| 15 | The Devil | Corruption: spreads Corruption biome tiles outward and **hijacks your bots** — reprogrammed veterans fight for it, XP intact. | Hijack-capable; terrain-altering |
| 16 | The Tower | Ruin: ignores your bots entirely; sudden all-in lightning raids on structures — Fabricators and Archives first. | Static siege scripts, long dormancy between strikes |
| 17 | The Star | Guidance: relay beacons that extend **other nests'** broadcast range and repair their units. Kill the support first. | Static, cross-nest cooperative |
| 18 | The Moon | Illusion: decoy units running deliberately misleading (but real, fully readable) programs; forges **fake Black Boxes** with lying logs. Trust nothing on this part of the map. | **Procedural counter-intel**; code is open and dishonest |
| 19 | The Sun | Clarity: no tricks — simply the best straightforward combat programs in the game, surging on full Energy. Honest and terrifying. | Static, peak-quality authored code |
| 20 | Judgement | Resurrection: reboots its dead **with XP intact** — its veterans accumulate all match. Leave no wrecks, or face them again, stronger. | Static; XP-preserving reprints |
| 21 | The World | Completion: rotates through the behaviors of every lower arcanum and uses the full construct set. The endgame nest. | **Researches + procedurally mutates**; everything |

### What Allegiance controls (the mechanical flags)

- **Program quality**: which construct tiers ([01-language.md](01-language.md)) and function blocks its scripts use. Roughly: arcana 0–4 preview Tiers 0–2, 5–13 preview Tiers 3–5, 14+ use things players are still saving Data for.
- **Code modification** (your Magician instinct, generalized): `static` (most) / `mutates-per-print` (1, 10, 18, 21) / `researches` (14, 21 — these escalate their own tree over the match, answering "should nests research?": *some do, by arcanum*).
- **Mutation style**: authored variants vs. procedural — set **per nest type and biome**. A Magician nest in Corruption mutates handlers; one in a Loop Desert unrolls loops. Biome cost overlays ([05-terrain.md](05-terrain.md)) shape what mutations are *viable*, so the same arcanum plays differently across the map.
- **Map placement**: start zones see 0–4; the midfield 5–13; deep field and co-op endgame events 14–21. Allegiance is geography as much as clock.

## Capturing Ferals (decided)

`hijack()` (late-game function block, [06-progression.md](06-progression.md)): field-repair an enemy wreck during its self-destruct countdown while flashing one of your **color programs** onto it — it passes through the standard Boot Sequence ([02-agents.md](02-agents.md)) and comes up as *your* bot, Feral chassis and all. Boot-as-interrupt applies: a hijack under fire explodes the prize. The Hierophant (5) and the Devil (15) run the same play against *you* — protect your wrecks or lose them twice.

## Escalation

```mermaid
flowchart LR
    T0[Calm<br/>Drones only] --> T1[Probing<br/>Stingers + Harvesters]
    T1 --> T2[Contested<br/>Wardens, coordinated raids]
    T2 --> T3[Overrun<br/>program VARIANTS appear]
    T0 -.->|player expansion,<br/>noise, Nest proximity| T1
```

- Escalation is driven by **player footprint** (territory claimed, energy output, Ferals killed), not wall-clock — turtles stay calm, expanders get pressure. Escalation and Allegiance are orthogonal: **allegiance is who a nest is; escalation is how awake it is.** A provoked Fool nest just sends more fools; a provoked Magician mutates faster.
- **Variants**: at high threat, nests with the mutation flag print archetypes with *modified programs* (e.g. a Stinger whose flee threshold is removed). Variants are flagged visually; the Codex diff view shows exactly what changed. Late-game reading comprehension test.
- **Handler-tier Ferals**: the Stinger polls `if health_low():` — deliberately the *worse* pattern. Higher-tier variants replace it with an `on hurt:` handler (retreat fires instantly, mid-chase), previewing the signal-handler unlock ([06-progression.md](06-progression.md)) and demonstrating exactly why it's better: you watch a variant Stinger break off the *instant* your first shot lands.

## Co-op & PvP Role

- **Co-op**: Ferals are the primary antagonist; escalation scales with combined player footprint.
- **PvP**: Ferals are map hazard + neutral economy (deny opponents Data by controlling Nest kills). Optionally disabled in "pure" PvP.

## Decided

- **Capture & reprogram: yes** — `hijack()` via the Boot Sequence (see Capturing Ferals). Hierophant and Devil nests mirror it against players.
- **Hijacked Ferals keep their XP** — high-arcana veterans are the best capture targets, mirroring Judgement's XP-keeping resurrections.
- **Some nests research** — controlled by arcanum (Temperance, The World); the rest are static or mutate-only.
- **Mutation style is per nest type × biome** — authored vs. procedural is an arcanum flag, flavored by the biome's cost overlays.
- **Nest Allegiance 0–21** (Major Arcana) is the enemy difficulty-and-personality axis; number ≈ difficulty, arcanum ≈ how it treats code.
- **Controlled nests gate printers/colors** (quadratic) — see Nest section above.
- **v1 arcana subset: 0 (Fool), 1 (Magician), 5 (Hierophant), 7 (Chariot), 13 (Death), 16 (Tower), 18 (Moon)** — spans the difficulty axis and covers the flag matrix: static, mutating, hijacking, salvage-denial, siege, and counter-intel.
- **Losing a claimed nest makes its printer dormant, not dead** — desired max forced to 0, color frozen (no prints, no hotfixes); stragglers optionally absorbed by other printers or left as a ghost fleet. Retaking the nest reactivates it ([01-language.md](01-language.md)).

## Open Questions

- Do high-arcana nests (14+) appear in PvP maps as neutrals, or are they co-op-only? A Devil nest hijacking *both* sides' wrecks could be a great PvP destabilizer.
- Procedural mutation guardrails: mutated programs must stay parse-valid and non-degenerate (no fault-looping armies unless the arcanum *wants* that — looking at you, Fool).
- Can Ferals *attempt* to re-take claimed nests proactively (siege arcana like The Tower targeting them), or only reclaim undefended ones? Lean: arcanum-dependent — Tower and Justice besiege, others only opportunistically reoccupy.
