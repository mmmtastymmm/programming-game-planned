*Part of [04-enemies](../04-enemies.md).*

# Feral Archetypes (initial set)

Each archetype = chassis + program. Programs shown are their *actual* shipped source — legal Pyrite: Feral programs run with **nest-bound values** — `home` (their own nest) and `patrol_route` — pre-bound at print (the kind-constant mechanism, faction-scoped — Q79), and `deposit()` treats their nest as their depot.

(The **Nest** itself — the printing structure and the territory game around it — is owned by [nests-and-claims.md](nests-and-claims.md).)

**Bind once, never check-then-act** (Q110, ruled inside Q117's answer — [history/questions-answered.md](../history/questions-answered.md)): a shipped source binds its target once rather than re-querying it around a blocking verb, whose tens-of-ticks window makes the race systematic ([01-language/syntax-tiers.md](../01-language/syntax-tiers.md) accepts only the *adjacent-ops* guard race). The Drone's and the Stinger's paired `closest(enemy)` calls below predate the ruling and still show the racing form — registered as [PROBLEMS.md](../PROBLEMS.md) P16; the Harvester's own staleness is P10.

## Drone (threat 1) — teaches Tier 0

```python
wander()
wander()
wait(3)
if exists(enemy):
    move_to(closest(enemy).expect())
    attack(closest(enemy).expect())
```

Harmless in ones. Exists so the first program a player ever reads is trivially comprehensible. The `move_to` before the swing is load-bearing (Q108): `attack()` on a non-adjacent target faults, so without it the Drone crash-loops the moment it *sees* an enemy — and the first program a player reads must not teach a bug they would copy. The `wait(3)` gives the Magician's mutation an integer literal to bite on.

## Stinger (threat 2) — teaches conditionals

```python
if health_low():
    move_to(home)
    wait(8)
if exists(enemy):
    move_to(closest(enemy).expect())
    attack(closest(enemy).expect())
wander()
```

Counterplay written in the code: hurt it and it *will* run — ambush the retreat path.

## Harvester (threat 2) — economic enemy

```python
if exists(ore):
    vein = closest(ore).expect()
    move_to(vein)
    mine()
    move_to(home)
    deposit()
wander()
wait(4)
```

The `exists(ore)` guard is load-bearing (Q108): without it a worked-out map turns every Harvester into a crash-loop rather than an enemy. Steals *your* map's ore and feeds its nest. Ignores bots entirely — a pure race pressure on the economy.

## Warden (threat 3) — teaches loops + messaging

```python
for spot in patrol_route:
    move_to(spot)
    if exists(enemy):
        target = closest(enemy).expect()
        try_broadcast("intruder", target)
        move_to(target)
        attack(target)
wait(6)
```

Patrols and *calls for help* (other Ferals block on `receive("intruder")`). Counterplay: jam or bait the call, or kill it inside one patrol leg.
