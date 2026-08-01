*Part of [08-multiplayer](../08-multiplayer.md).*

# Modes (DECIDED: one model — allied colonies, interaction is up to the server)

**Every player owns their own colony.** There is no shared-colony mode: co-op vs. PvP isn't a hard mode split but *how players choose to interact on a given server*. Allies share research intel, program libraries, and leak intel; rivals fight. The same match can contain both.

| Server setting | Effect |
|---|---|
| **Open** (default) | Players may ally, trade, raid, or war freely. Ferals escalate against everyone. |
| **Non-PvP** | Players **cannot directly harm each other** (no damage to other players' bots/structures; no salvaging, `analyze()`-ing, or hijacking their wrecks/units — Q76). One physics exception: **wreck blasts hit friend and foe on every server type** (Q55) — standing near anyone's countdown is on you, and an "ally" who walks ticking wrecks into your base is answered socially. Competition is indirect: territory, nests, resources. Ferals remain the common enemy. |
| **Duel** (stretch) | 2 players, tiny mirror map, fixed identical loadouts, pure program-vs-program. Esports-minimal; also the perfect balance-testing arena. |

- **PvP entry gate**: joining any server where players can be harmed requires **all language constructs permanently unlocked** ([06-progression.md](../06-progression.md)) — every combatant has the full language; matches are decided by usage, not vocabulary. Non-PvP servers have no gate.
- Allied-colony scaffolding: shared **program library** (call a friend's published functions), shared color-decryption intel **from the alliance forward only** (Q107: pre-alliance levels never merge — decryption is permanent and monotonic, so a merge-on-formation would let a faction ally for one tick, absorb everything a partner ever learned, and divorce; forward-only pooling leaves nothing to unwind), grantable channels and vision — but **not shared progression**: each colony recovers its own Function Caches and earns its own unlocks ([06-progression.md](../06-progression.md)). Allies share *work products*, not capability.
