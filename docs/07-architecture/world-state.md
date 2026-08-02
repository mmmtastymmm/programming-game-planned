*Part of [07-architecture](../07-architecture.md).*

# Key World-State Shapes (sketch — plain structs + `BTreeMap`s, not ECS)

```text
Bot entity:      BotId, Hp, Cargo(typed manifest), Modules, XpTracks,
                 Env, QuirkList+QuirkScratch, VmState, DeployedProgram,
                 TilePos, Faction
World extras:    per-tile counters (scree, dune sink, Corruption spread),
                 Hiding-episode state, allocation table (assignments +
                 check timer + pending polite recalls), comm keys per
                 faction, colony stock, structure buffers/pads + recipe
                 selections, wreck countdowns, per-faction node map
Structure:       StructureKind, Hp, Buffers, TilePos, Faction
Tile map:        dense Grid<TileKind> world field + spatial index (bots per tile)
Programs:        ProgramLibrary table — source + AST, shared/refcounted
                 (100 bots on one program share one AST)
Commands:        DeployProgram, QueuePrint(faction), PlaceBlueprint
                 (structures, terraform, repairs), EditPrinterRules
                 (targets, keys, directions, priority, check interval),
                 QueueUpgrade(bot, catalog item — the program must bring
                 the bot to a pad; the pad applies queued orders),
                 SetRecipe(structure, recipe),
                 PlaceOverlay(arrow — instant signage),
                 PlacePaint(pos, color|unpainted — Q97: places a paint
                 DESIGNATION a bot services, the blueprint flow),
                 ClaimNest(nest), RazeNest(nest) — on a DEFEATED site:
                 claiming converts it, razing banks its Data bounty (Q86),
                 ExchangeData, PostRequest,
                 Grant(faction, channel | vision | module), SetAlliance,
                 Vote(sim-speed | decommission), Research(UnlockId)
                 — the ONLY external inputs to sim (Q77: list completed;
                 it grows only when a decided system adds a player input)
```
