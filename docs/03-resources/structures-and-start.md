# Structures, Ally Aid & Starting State

*Part of [03-resources](../03-resources.md).*

## Structures & Starting State


## Ally Aid: the Request Box

No free-form resource gifting. A colony builds a **Request Box** and posts a request on it (*resource, amount*). Allied bots may — entirely voluntarily — haul the requested resource in and `deposit()` it; the owner collects what arrives.

- Aid is **physical logistics**: someone's haulers cross the map to deliver it, through whatever is between the colonies. Charity has supply lines.
- It's **programmable in spirit, post-v1 in letter** (Q79): allies fulfill requests today by reading the box in the UI and hauling with plain `deposit()`; the request-reading builtins that would make generosity a standing program are deferred.
- Requests are visible to all allies (and, being on the field, spottable by enemy scouts: a colony begging for Steel is telling everyone something).

## Structures (resource-relevant set)

**Every structure is built by labor** (Q105, 2026-07-26): the player designates a blueprint, a bot walks there and `build()`s it, exactly like bridges, roads, barricades, and paint. Nothing in the colony appears the instant it is clicked — "designation is the player's; labor is code" holds everywhere, and a contested build is a thing you defend.


| Structure | Cost | Function |
|---|---|---|
| **Fabricator** (printer) | 20 Steel | Prints/reprints bots for **one program color** ([01-language.md](../01-language.md)); buildable count gated by controlled nests ([04-enemies.md](../04-enemies.md)). Each adds a fixed amount to the colony's **fleet cap**; printers after the first carry a **target share + selection key** for which bots wear their color (the first takes the remainder), enforced by recall ([01-language.md](../01-language.md)). Loses its backing nest → **dormant**: cap contribution withdrawn, target voided, color frozen. Printers are also **the cloud**: they always accept `upload_log()` / crash-dump traffic (even dormant), and any printer's inspector is the colony's telemetry viewer — color-coded and filterable by log level ([01-language.md](../01-language.md)). The colony heart; losing your last one is near-lethal — and it takes your telemetry with it. Also a **backup generator** (Q84): its trickle always keeps **one bot** powered at full budget (deterministic pick: lowest entity ID) — a total blackout can never deadlock the colony, because someone can always walk out for fuel. |
| **Depot** | 5 Stone | Cargo drop-off, storage. |
| **Smelter** | 10 Steel | The heat works: **2 Iron + 1 Coal → 1 Steel**, **1 Copper + 1 Tin → 1 Bronze**, or **2 Sand → 1 Glass** (recipe set per Smelter via the `SetRecipe` Command — round 4). Needs energy. |
| **Foundry** | 25 Steel, 10 Bronze | The precision works: **1 Copper → 1 Wire**, **1 Silver + 2 Crystal + 1 Wire → 1 Chip**, **2 Glass → 1 Lens**, or **1 Chip + 1 Gold → 1 Gold Chip** (recipe set per Foundry via `SetRecipe`). Needs energy. Priced in Smelter goods only (Q72): **expensive, never impossible** — the electronics age is a big Steel-and-Bronze investment, not a chicken-and-egg. |
| **Generator** | 8 Steel | Burns fuel → Energy rate: Wood (weak) or Coal (strong). Fed physically (`deposit()` fuel into its intake — Q79/Q84). |
| **Geothermal Tap** | 12 Steel | Free steady Energy, only on vent tiles. |
| **Pump** | 6 Steel, 2 Wire | **Two tiles** (Q98, the only multi-tile structure): an **intake** standing in a Water tile — river, lake, or coast, any water is a source — and the **pump house** on orthogonally adjacent walkable ground. Extracts Water into the house's output buffer at a steady rate (tuning) for bots to `withdraw()` and haul. The only source of coolant. Bots deal with the house; the intake is solid, so **you cannot bridge through a pump** — waterfront becomes a footprint that pumps, bridges, and rival pumps compete for, and a visible tell of where a colony draws its coolant. |
| **Research Archive** | 10 Steel, 5 Stone | The **Data Exchange** (Q106: ships in v1) — Data → other resources at a **flat rate table in data** (everyone, forever; Chips-favored, **Gold trades best per unit**; no scarcity scaling until playtest says otherwise), requiring a built Archive — and the colony's telemetry annex. The Exchange is what keeps Data worth earning all match: construct research is *finite*, so without it a colony that finishes researching keeps earning a currency it can never spend. **Construct research needs no structure** (Q84): it's a player Command spending colony Data, so learning is never gated behind building — the Archive is the bank, not the school. |
| **Repair Bay** | 8 Steel | Repairs bots in range (energy drain while active). The target of hurt-handler retreat programs ([01-language.md](../01-language.md)). |
| **Upgrade Station** | 10 Steel, 5 Chips, 3 Wire | **Your first Station is the ruin in the start base** (P1 ruling — see Starting State); this build price buys *additional* Stations, affordable once the ruin's repair opens the Chips chain. Bots walk here to buy **per-bot upgrades** — every **tool grade** (one tool per XP track, grades 2–5, licensed by level — Q111/Q118) plus the flat capacity buys (memory, stack, log buffer); catalog in [06-progression.md](../06-progression.md). **Only the compute family draws Water as coolant** (Q119): the CPU tool and the capacity buys are silicon and genuinely thermal, while drills, weapons, plating and the rest are mechanical and pay none. Whether an entry needs coolant is declared **per catalog entry in data**, never inferred from which code path handles the purchase — M16 attached the charge to a code branch, a later entry inherited it, and buying a *drill* silently required a water chain the colony had no way to know about. Works like a printer (Q68, decided): the bot **mounts the pad, sits inert for the upgrade time** (tuning, per catalog entry), and steps off upgraded — one pad, one bot, so the queue is physical and everyone in it is exposed. Orders are player-queued per bot (a `QueueUpgrade` Command — designation is the player's); the **program** must bring the bot to a pad. **Bots never path onto the pad** (Q84): they stand adjacent, and the pad *pulls* the next adjacent bot with a queued order (lowest entity ID) — no crash-looping on an occupied tile. Payment charges **at mount**; an unaffordable order is **skipped** — the pad moves to the next queued bot and the skipped order re-arms when stock covers it (no livelock, round 4). The pull silently cancels the bot's pending action (no signal); pad-sit is an **interrupt context** (double-handle applies — upgrading under fire risks the prize); stepping off restarts the program at line 1 (no boot — no re-coloring happened). Player-placed like any structure; parking your veterans on a pad in contested ground is a choice. |
| **Sentry Post** | 4 Stone, 1 Glass | Wide sensor radius, nothing else. Fog of war is eyes-only ([05-terrain.md](../05-terrain.md)) — fixed sightlines are cheap infrastructure, but even a watchtower needs a window. |
| **Lantern** | 2 Wood | Tiny fixed sensor radius (~2 tiles, tuning) — a light, not a watchtower. The cheapest ward against eyes-only fog: string them along perimeters and haul roads. Its little seeing circle is real sight (Q74): lit ground is surveyed ground, and a mover crossing it is detected. |
| **Request Box** | 3 Stone | Posts a resource request allies may voluntarily fulfill by hauling and depositing (see Ally Aid). |

## Starting State (per player)

- 1 working Fabricator (the **Green** printer), 1 **ruined Red Fabricator** (repairable for Data — the first colony milestone, [01-language.md](../01-language.md)), 1 **ruined Upgrade Station** (repairable for **tier-0/1 materials** — tuning; the P1 ruling, same pattern as the Red Fabricator — without it the buildable Station's Chips price sits above the drill ladder it sells), 1 Depot, 1 Generator
- 2 bots (Green, **tier-1 mining tools** slotted — the start-zone veins are Iron and Coal, tier 1) with a working starter mining program pre-deployed (the tutorial *is* reading this program)
- 30 Steel, 10 Iron + 5 Coal in colony stock, 0 everything else (map generation guarantees Iron + Coal + Wood + Stone in the start zone; Copper/Tin within first-expansion reach — Q69)
- **The Generator starts stoked** (Q84 — a tuning buffer of fuel), and the start guarantee includes *starting upkeep < starting generation*: the opening never brownouts before the player acts. The starter economy's first real job is keeping it fed.

