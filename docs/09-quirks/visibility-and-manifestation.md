*Part of [09-quirks](../09-quirks.md).*

# Visibility, Introspection & Manifestation

## Visibility & intel

XP levels are visible to everyone as wear/decals (pillar 2); quirks show the same way (Q47, decided): **enemy-readable for free** — a Loud Fans bot audibly *is* loud, and the juicy-corpse precedent ([02-agents.md](../02-agents.md), Q68) already committed to physical facts being legible. No decryption gate. `my_quirks()` / `has_quirk()` are free of any unlock whenever quirks are enabled (Q48, decided) — a rolled quirk nobody's code can read is pure noise, and per-bot adaptation ([design rule 4](design-rules.md)) is the whole payoff. **Latent quirks are invisible to everyone, introspection included** — until the XP threshold, the quirk does not exist to the world or to the bot itself.

## Determinism & data notes

- Roll at print time, `quirk_roll` RNG stream, weighted by `quirks.ron` rarity — but rolls are **latent**: a quirk manifests (effect + visibility + introspection) only when the bot's **Age LEVEL** reaches its threshold (Q105 ruling (a), restated 2026-07-28 — see [decided.md](decided.md)). Manifestation is a deterministic threshold check, no RNG.
- Reprint = new body = new roll (latent again). Recall/re-coloring and rescue keep the body, so quirks — manifested or latent — persist (like XP, they live on the bot, not the color).
- Golden-replay note: quirks change state hashes; they live behind the quirk-probability match setting (0 = off, [08-multiplayer.md](../08-multiplayer.md)), so hash-affecting content is always gated.
