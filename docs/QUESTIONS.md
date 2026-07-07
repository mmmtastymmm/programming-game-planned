# Open Questions Worksheet

All design questions collected from docs 01–08. As each is decided, it moves to the owning doc's *Decided* section and is marked answered here. Numbering is stable — append new questions, never renumber.

**Status 2026-07-06: Q1–Q34 ALL ANSWERED.** Nothing awaits a design decision. Two playtest-tuning items remain (they need the prototype, not a choice): upkeep mix balance (02) and Corruption spread/cleanse rates (05).

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
