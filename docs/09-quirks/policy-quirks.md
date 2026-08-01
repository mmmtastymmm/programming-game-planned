*Part of [09-quirks](../09-quirks.md).*

# Policy Quirks Ride the Environment

Some quirks need no stat plumbing at all. Any quirk whose effect is "*when* does an engine behavior fire" is really a modified **env registry** entry ([01-language.md](../01-language.md), The Environment): Defensive Programming is just a bot that ships with a higher `hurt_line`. Two strengths, declared per quirk in `quirks.ron` (Q60, decided):

- **Temperament — a shifted default.** The key's *default* changes (unset `hurt_line` reads 60, not 50). Programs that never touch the key inherit the personality; one `setenv` in the boot window overrides it entirely. Temperaments tax only unwritten code — the quirk is real on day one and evaporates under a good dotfile, which is about as "code is the game" as a quirk can get.
- **Compulsion — a clamped range.** The key's legal *range* narrows (`hurt_line` 55–99). Decided semantics: `setenv` past an *engine* bound still faults (that's a program bug, identical on every bot), but `setenv` past a *quirk* clamp **clips** quietly — the hardware refuses, deterministically, and `getenv` reports where the value actually landed. One color program stays valid on every bot; the compelled bot just can't be talked out of its fear.

Every future env key is free quirk surface — the registry is the natural home for personality, and `getenv` doubles as quirk introspection for these (relevant to Q48).
