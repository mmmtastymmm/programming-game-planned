# Open Questions Worksheet

All unresolved design questions collected from docs 01–08. Answer inline under each **Answer:** or reference the numbers in conversation. As each is decided, it moves to the owning doc's *Decided* section and gets removed here.

---

## Language (01)

*Q1–Q3 answered 2026-07-06 (blocking channels; hurt threshold as unlock; log buffer = hardware). New question below.*

### Q25. Channel espionage
Channel names leak with code (salvage decryption). On harm-enabled servers, can enemies `receive` on stolen channels (eavesdrop — and in one-receiver mode, *steal* the message before its intended recipient) or `send`/`broadcast` (spoof commands)?
**Lean:** yes to all — leaked code should leak infrastructure; protocol defense (rotating names, enum-tag auth) becomes endgame craft.
**Answer:**

## Agents (02)

*Q4–Q5 answered 2026-07-06 (upkeep is a data-driven mix, v1 = Energy + Metal; wreck countdown scales with XP).*

## Resources (03)

*Q6–Q8 answered 2026-07-06 (regen is a per-node-type data flag, mostly off; tiered harvest tools, Crystal high-tier; ally aid via Request Box, voluntarily fulfilled by hauling).*

## Enemies (04)

*Q9–Q11 answered 2026-07-06 (max arcanum is a match option, high nests spawn farther out; mutations stay functional; Ferals reclaim nests).*

## Terrain (05)

*Q12–Q14 answered 2026-07-06 (terraforming in scope, build + deconstruct; fog of war is eyes-only; Corruption radiates from sources and regrows unless rooted out). New question below.*

### Q29. Do Barricades block vision?
Movement-only walls, or sensor-blocking too (true walls)? Vision-blocking changes the eyes-only fog game substantially.
**Lean:** movement-only.
**Answer:**

## Progression (06)

*Q15–Q18 answered 2026-07-06 (progression fully per-player, allies share work products not capability; ALL function blocks findable at Caches near base; any PvE earns constructs; prereqs per-player). New questions below.*

### Q27. Veteran Data sinks
With constructs owned (permanent) and functions found (Caches), what does Data buy a veteran mid-match? Candidates: cache-locator pings, Archive boosts, reprint discounts.
**Lean:** needs *something* — Data shouldn't go dead for the players who generate the most of it.
**Answer:**

### Q28. Contested Caches
Rare mid-map single-copy Caches holding late functions (`hijack`)? Strong PvP objective; feel-bad risk in co-op.
**Lean:** yes, on Open servers only.
**Answer:**

## Multiplayer (08)

*Q19–Q20 answered 2026-07-06 (unanimous sim-speed votes with configurable cooldown; disconnected colonies run autonomously, remaining players can vote to decommission). PvP-disconnect handling split off below.*

### Q26. PvP disconnects
An autonomously-running colony with nobody at the helm is a free farm and a code-leak piñata. Grace-period armistice? Auto-forfeit window? Ally takes the helm on team servers?
**Lean:** none yet — needs a real answer before ranked play.
**Answer:**

### Q21. Indirect aggression on Non-PvP servers
Racing an ally to a nest claim, out-mining contested veins — allowed?
**Lean:** yes — that's the point of the setting.
**Answer:**

## Architecture (07)

*Q22–Q24 answered 2026-07-06 (bevy_ecs with sorted/ordered queries — rule enshrined in CLAUDE.md; A\* first with a flow-fields note; programs stored as byte-exact plain text, versions = source hash).*
