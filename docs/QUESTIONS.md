# Open Questions Worksheet

All design questions collected from docs 01–08. As each is decided, it moves to the owning doc's *Decided* section and is marked answered here. Numbering is stable — append new questions, never renumber.

**Status 2026-07-06: Q1–Q34 ALL ANSWERED.** Nothing awaits a design decision. Two playtest-tuning items remain (they need the prototype, not a choice): upkeep mix balance (02) and Corruption spread/cleanse rates (05).

**2026-07-12: Q35–Q40 opened** (new terrain brainstorm, owner: 05-terrain).

---

## Open

- **Q35 (Terrain — Sand).** Sand as a heavy-cost tile (~5×, tuning constant). Open: does it need a distinguishing hook beyond cost, since Mud already owns "slow tile" (3×, 4× loaded)? Candidate: **sand punishes stopping** — idling more than N ticks on sand sinks the bot (escalating exit cost or a `stuck` fault), so `wait(n)` staging and rally points become unsafe in dunes. Mud punishes hauling routes; sand would punish loitering.
- **Q36 (Terrain — Mountain edge costs).** Should movement cost become a function of the **edge** (`from → to`) instead of the destination tile? Proposal: climbing onto a mountain is expensive, descending moderately expensive, ridge-to-ridge is plains-speed — mountain ranges become highways with costly on-ramps. Interacts with planned **High Ground** (ramp-gated, +2 sensor): do soft-climb mountains replace it (keeping the sensor bonus on summits) or coexist as the harder variant? Note: edge costs touch A*/`move_to`, so golden replay hashes change when this lands.
- **Q37 (Terrain — Ice).** Slide tile: entering ice continues the move in the same direction until non-ice (deterministic, 1×/tile, uncontrolled). Programs must plan slide endpoints; mass-produces `on bump:` use. Open: how do slides interact with bump freezes and one-way arrow overlays mid-slide?
- **Q38 (Terrain — Ford).** Shallow water: passable at high cost (~4×), possibly sensor-silent while wading. Makes bridges an upgrade rather than a hard gate — smooths the pre-terraforming game. Open: does this undercut the bridge/chokepoint economy that Water is designed to create?
- **Q39 (Terrain — Roads + integer cost scaling).** Terraformed road tile at half plains cost (the "tech tile" ground art). Requires scaling the base cost table ×2 (Plains 2, Road 1, Rubble 4, ...) since costs are integers — a one-time costs.ron migration that also buys finer tuning granularity for everything else. Decide before more cost-sensitive terrain ships.
- **Q40 (Terrain — wear & traces).** Two related "terrain remembers traffic" ideas: **Scree** that collapses after N crossings (per-tile counter, like natural bridge HP) so optimal programs rotate routes; and **footprints on sand** — decaying marks a future `tracks_at()` sensor could read, turning scouting from "where are they" into "where were they." Open: is per-tile mutable traffic state acceptable sim-size-wise, and is `tracks_at()` a Tier-2 sensor or later?

---

## Answered log

- **Q1–Q3, Q25** (Language): blocking channels (2×2 send/broadcast × try variants); hurt threshold as unlock; log buffer = hardware stat; channel espionage legal but gated by **comm keys** + decrypted names.
- **Q4–Q5** (Agents): upkeep is a data-driven mix, v1 = Energy + Metal; wreck countdown scales with XP.
- **Q6–Q8** (Resources): regen is a per-node-type data flag, mostly off; tiered harvest tools (Crystal high-tier); ally aid via Request Box, voluntarily fulfilled by hauling.
- **Q9–Q11** (Enemies): max arcanum is a match option, higher nests spawn farther out; mutations stay functional; Ferals reclaim nests.
- **Q12–Q14, Q29** (Terrain): terraforming in scope (build + deconstruct + cleanse); fog of war is eyes-only; Corruption radiates from sources; tall things block vision (Barricades are true walls, High Ground sees over).
- **Q15–Q18, Q28** (Progression): progression fully per-player; ALL function blocks learned at Template Caches (non-consumable schools, contest is territorial); any PvE earns constructs; prereqs per-player.
- **Q19–Q21, Q26** (Multiplayer): unanimous sim-speed votes with configurable cooldown; disconnected colonies run autonomously + decommission vote; indirect aggression always legal; PvP disconnect = free farm until reconnect.
- **Q22–Q24** (Architecture): `bevy_ecs` with sorted queries (rule in CLAUDE.md); A\* first, flow-fields note; byte-exact plain-text programs, version = source hash.
- **Q27** (Data): Data is a **currency** — the Research Archive runs a Data Exchange (Data → Chips/Metal, tuned rates). Never goes dead for veterans or in PvP.
- **Q30**: players **can hijack other players' wrecks** — XP-intact veteran theft is the third arm of the wreck race.
- **Q31 (revised)**: **Feral code is NOT open** — it decrypts by the same salvage attrition as player code, at per-arcanum rates (Fool leaks in ~2 kills; high arcana stay cryptic). Feral channels additionally require the nest's comm key via `analyze()`. One universal rule: programs are read on murder.
- **Q32**: full code disclosure **only after a match completes** (replays unlock post-match); live spectators see per-faction views.
- **Q33**: hijacked units are **never reprintable** — unique prizes.
- **Q34**: grace window stays in **ticks** — faster CPUs earning richer error handling is an intended hardware benefit.
