*Part of [04-enemies](../04-enemies.md).*

# Decided

- **Capture & reprogram: yes, any wreck** — `hijack()` works on Feral *and* player wrecks (harm-enabled servers) via the Boot Sequence (see [capturing-wrecks.md](capturing-wrecks.md)). Hierophant and Devil nests mirror it against players.
- **Hijacked units keep their XP and are never reprintable** — unique prizes; high-arcana veterans are the best capture targets, mirroring Judgement's XP-keeping resurrections.
- **Nothing Feral is free** — code decrypts by salvage/analyze attrition at per-arcanum rates (Fool leaks in ~2 kills; high arcana stay cryptic); channels additionally require the nest's comm key. One universal rule: programs are read on murder.
- **Some nests research** — controlled by arcanum (Temperance, The World); the rest are static or mutate-only.
- **Mutation style is per nest type × biome** — authored vs. procedural is an arcanum flag, flavored by the biome's cost overlays.
- **Nest Allegiance 0–21** (Major Arcana) is the enemy difficulty-and-personality axis; number ≈ difficulty, arcanum ≈ how it treats code.
- **Controlled nests gate printers/colors** (quadratic) — see [nests-and-claims.md](nests-and-claims.md).
- **v1 arcana subset: 0 (Fool), 1 (Magician), 5 (Hierophant), 7 (Chariot), 13 (Death), 16 (Tower), 18 (Moon)** — spans the difficulty axis and covers the flag matrix: static, mutating, hijacking, salvage-denial, siege, and counter-intel.
- **Losing a claimed nest makes its printer dormant, not dead** — fleet-cap contribution withdrawn, target voided, color frozen (no prints, no hotfixes); its bots become **ghost machines**: off the allocation, running frozen code, still drawing upkeep, dying by attrition (Q65). Retaking the nest reactivates the printer and **uploads its surviving ghosts back into the fleet** ([01-language.md](../01-language.md)).
- **The dormant printer is the one bound to the lost nest** (2026-07-17, answers Q87). Every **over-base** printer (the 3rd color onward — the ones the quadratic nest-gate lets you build) **records the nest it was built against** at placement. When that nest reverts (Ferals reclaim it) and the colony drops below the threshold, *that exact printer* goes dormant (`PrinterState::Dormant`) — not the newest, not an arbitrary pick. The two free base slots (colors 1–2) are never nest-bound and never dormant; the remainder color among them is additionally indestructible ([01-language.md](../01-language.md), Q88). Re-claiming the bound nest clears dormancy.
- **Max arcanum is a match option, on any server type** — higher-arcana nests always spawn farther from player starts; raising the cap deepens the frontier rather than hardening the neighborhood.
- **Mutated programs stay functional** — procedural mutation must yield parse-valid, non-degenerate programs. Buggy Feral code (the Fool) is an authored choice, never a mutation accident.
- **Ferals reclaim claimed nests** — claims must be defended; loss sends the printer dormant. Siege arcana (Tower, Justice) assault defended claims; others reoccupy empty ones.
