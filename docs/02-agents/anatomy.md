*Part of [02-agents](../02-agents.md).*

# Anatomy

Every bot is a **chassis + modules + program** — and every chassis is **identical**:

| Part | What it determines |
|---|---|
| **Chassis** | The universal printed body ([03-resources.md](../03-resources.md)) — **no classes**. Every bot prints with the same floor statline (below); everything above the floor is earned (XP), slotted (modules), or rolled (quirks). |
| **Capacity upgrades** | Program memory, variable slots, stack depth and log buffer — flat per-bot buys at an **Upgrade Station** ([03-resources.md](../03-resources.md)), a player-placed structure the bot must physically walk to. **Cycles per tick is the CPU tool**, licensed by the Processing track like any other tool (Q111). |
| **Tools** | **Ten tools, one per XP track** (Q111, 2026-07-27; supersedes capability slots, which superseded generic module slots) — drill, build tool, weapon, optics, CPU, cargo rack, hull plating, drivetrain, signature dampener, gyros. Each has **grades 1–5**; grade 1 ships free with the chassis and 2–5 are bought at an Upgrade Station. A grade is **licensed by level**: a bot may buy grade N once *either* that track's level *or* its total level reaches N. Tools carry the power; levels license it. |
| **Program** | One of the colony's **colored program slots** ([01-language.md](../01-language.md)) — one color per Printer, printer count gated by controlled nests. The bot is visibly tinted by its color. Redeploying a color updates all its bots at their next loop boundary; printer count sets the colony's **fleet cap**, and each printer after the first carries a **target share + selection key** choosing which bots wear its color (the first takes the remainder), enforced via the recall interrupt ([01-language.md](../01-language.md)). |

The universal base statline (the floor — roughly the worst of every option from the old class table; all tuning):

| Stat | Base |
|---|---|
| HP | 40 |
| Move rate | 14 ticks/tile (slow) |
| Cargo | 4 |
| Sensors | 5 tiles |

**Identity is earned, not printed.** A fresh print is slow, fragile, dim-eyed, and nearly empty-handed — the same sorry machine every time. What it becomes is written by what it does — and by simply lasting: XP tracks grow the body (HP by Age, speed by Mileage), total XP builds out the frame (slots), modules extend it, quirks bend it. The old sensing/carrying/surviving triangle didn't disappear — it moved from a print-time class picker to a lifetime of behavior. Identical rookies are the point: divergence starts at the first tick, and the print-time identity choice relocated to the first module + the color.

