# Open Questions Worksheet

All unresolved design questions collected from docs 01–08. Answer inline under each **Answer:** or reference the numbers in conversation. As each is decided, it moves to the owning doc's *Decided* section and gets removed here.

**Status 2026-07-06: Q1–Q26, Q28–Q29 all answered.** Remaining: **Q27** below, plus two playtest-tuning items (upkeep mix, 02; Corruption rates, 05) that need the prototype, not a decision.

---

## Language (01)

*Q1–Q3, Q25 answered 2026-07-06 (blocking channels; hurt threshold as unlock; log buffer = hardware; channel espionage fully legal — eavesdrop, message-theft, spoofing).*

## Agents (02)

*Q4–Q5 answered 2026-07-06 (upkeep is a data-driven mix, v1 = Energy + Metal; wreck countdown scales with XP).*

## Resources (03)

*Q6–Q8 answered 2026-07-06 (regen is a per-node-type data flag, mostly off; tiered harvest tools, Crystal high-tier; ally aid via Request Box, voluntarily fulfilled by hauling).*

## Enemies (04)

*Q9–Q11 answered 2026-07-06 (max arcanum is a match option, high nests spawn farther out; mutations stay functional; Ferals reclaim nests).*

## Terrain (05)

*Q12–Q14, Q29 answered 2026-07-06 (terraforming in scope; fog of war is eyes-only; Corruption radiates from sources; tall things block vision — Barricades are true walls, High Ground sees over them).*

## Progression (06)

*Q15–Q18 answered 2026-07-06 (progression fully per-player, allies share work products not capability; ALL function blocks findable at Caches near base; any PvE earns constructs; prereqs per-player). New questions below.*

### Q27. Veteran Data sinks
With constructs owned (permanent) and functions found (Caches), what does Data buy a veteran mid-match? Candidates: cache-locator pings, Archive boosts, reprint discounts.
**Lean:** needs *something* — Data shouldn't go dead for the players who generate the most of it.
**Answer:**

*Q28 answered 2026-07-06: Caches are non-consumable schools — anyone can learn from any site; contest is territorial (holding access), never exclusive. Renamed Template Caches.*

## Multiplayer (08)

*Q19–Q21, Q26 answered 2026-07-06 (unanimous sim-speed votes with cooldown; disconnected colonies run autonomously + decommission vote; indirect aggression always legal; PvP disconnect = free farm until reconnect — your code is your defense).*

## Architecture (07)

*Q22–Q24 answered 2026-07-06 (bevy_ecs with sorted/ordered queries — rule enshrined in CLAUDE.md; A\* first with a flow-fields note; programs stored as byte-exact plain text, versions = source hash).*
