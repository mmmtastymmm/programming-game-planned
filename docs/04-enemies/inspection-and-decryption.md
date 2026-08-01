*Part of [04-enemies](../04-enemies.md).*

# Why Ferals Run the Player's VM, and How You Read Them

The core conceit (requirement 5): **Ferals run real Pyrite programs on the same VM as player bots**, and players can *decrypt and read those programs* — by the same rule that governs everything: programs are read on murder.

## Why enemies run the player's VM

- **One interpreter, one truth.** No separate AI system to build or keep deterministic ([08-multiplayer.md](../08-multiplayer.md)). Feral behavior is exactly as inspectable, steppable, and deterministic as player code.
- **Reading code is the counterplay.** A Feral's program is its stat block *and* its weakness. `if attacker_count > 2: move_to(home)` is an instruction to the player: bring three bots. Decryption is how you earn the read.
- **Enemies are the curriculum.** Early Feral programs are simple Tier-0/1 scripts that teach by example (and leak in a kill or two); late ones use constructs the player hasn't unlocked yet — a preview of their own future power, behind a longer decryption grind.

## Inspection & Decryption

Feral programs are **encrypted exactly like player code** ([08-multiplayer.md](../08-multiplayer.md)): each salvage/analysis of a nest's unit grants **permanent +N% decryption** of that nest's archetype program. One universal rule, no exceptions: **programs are read on murder** — yours, theirs, everyone's.

| Method | What you get |
|---|---|
| Click any visible Feral | Archetype + nest tag, live behavior — and your current **decrypted view** of its source (stable noise where unrevealed), with **live program counter** stepping over the lines you've revealed |
| `analyze()` any wreck (Q76: the intel verb, player wrecks included) | **Data** ([03-resources.md](../03-resources.md)) + the wreck's **logs + env snapshot** + its faction's **comm key**; Feral wrecks add **+N% decryption** of that nest's archetype program ([01-language.md](../01-language.md)). Destroys the wreck — materials (`salvage`) or intel (`analyze`), pick one |
| Codex library | Every decrypted view, versioned and diffable (mutating nests create versions; your % persists across them) |

- **Decrypt rate is per-arcanum tuning** — the difficulty knob: the Fool leaks its whole program in a couple of kills (the curriculum still works; it's just earned), while high arcana stay cryptic across a long campaign.
- Once decrypted, the live program-counter view delivers the aha-moments: a retreating player literally watches the pursuer's code hit its `if leash > 40: move_to(home)` line.
- **Channels are never included**: even at 100% decryption you can *see* the Warden calls `try_broadcast("intruder", …)`, but listening in or spoofing requires the nest's comm key — reading is reconnaissance; interacting takes fieldwork. Suppressing a nest's alarms by message-stealing, or baiting defenders with fake alerts, is intended late-game play.
