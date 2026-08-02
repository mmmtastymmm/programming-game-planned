*Part of [06-progression](../06-progression.md).*

# The Unlock Tree

```mermaid
flowchart TD
    START([Game start:<br/>straight-line programs + <b>if / elif / else</b><br/>+ the start kit:<br/>move_to + try_move_to, mine + try_mine,<br/>deposit + try_deposit, withdraw + try_withdraw,<br/>study, closest, closest_minable, exists,<br/>exists_minable, wait, rng, drop_cargo, abort,<br/>wander, cargo_count, is_seen])

    subgraph Constructs["Language constructs (one-time Data cost, PERMANENT)"]
        VAR["Variables — 10"]
        WHILE["while / break — 35"]
        SIG1["on error: window — 40"]
        SIG2["on hurt: window — 55"]
        BUMP_H["on bump: / on bumped: windows — 30"]
        BOOT_W["on boot: window — 45"]
        DEF["def / return — 50"]
        IMPORT["import / from-import — 65"]
        LIST["lists, dicts + for-in — 60"]
        ENUM["enum + match — 70"]
        MSG_C["channels: send / receive — 80"]
    end

    subgraph Functions["Function blocks (found at Caches — number ≈ cache depth)"]
        F_SENSE["cargo_full, health_low,<br/>path_blocked — 5"]
        F_SEARCH["search, explore<br/>(the scouting stance) — 12"]
        F_LOG["log, upload_log, upload_crash_dump,<br/>recover_black_box, last_error — 10"]
        F_SALV["salvage — 18"]
        F_ATK["attack — 15"]
        F_BUILD["build, repair — 20"]
        F_SCAN["scan_enemies, scan_resources — 40"]
        F_AN["analyze — 30"]
        F_BC["send/broadcast + try variants,<br/>receive/try_receive — with channels"]
        F_GUARD["guard, escort — 45"]
        F_HIJACK["hijack — 70"]
        F_TERRA["terraform blueprints unlocked:<br/>clear, bridge, barricade, road,<br/>demolish, cleanse — 35"]
        F_ENV["setenv / getenv (env variables:<br/>hurt_line, log_min_level) — 25"]
    end

    START --> VAR
    START --> F_SENSE
    START --> F_LOG
    VAR --> WHILE
    VAR --> SIG1
    F_LOG --> SIG1
    SIG1 --> SIG2
    START --> F_ATK
    F_ATK --> F_GUARD
    F_ATK --> F_SALV
    START --> F_BUILD
    WHILE --> DEF
    DEF --> IMPORT
    START --> F_SEARCH
    F_LOG --> F_ENV
    SIG1 --> BUMP_H
    SIG1 --> BOOT_W
    F_BUILD --> F_AN
    F_BUILD --> F_TERRA
    F_AN --> F_HIJACK
    SIG2 --> F_HIJACK
    DEF --> LIST
    LIST --> F_SCAN
    LIST --> ENUM
    ENUM --> MSG_C
    MSG_C --> F_BC
```

**Program color slots are deliberately NOT in this tree** — they aren't researched with Data. Colors are gated by **controlled Feral nests** on a quadratic curve ([01-language.md](../01-language.md), [04-enemies.md](../04-enemies.md)): a third progression axis (territory) alongside research (Data) and hardware (the Upgrade Station).

Handler-window unlocks buy the right to **edit** that signal's window ([01-language.md](../01-language.md)) — pre-unlock, the reserved template still runs with its factory contents, so nothing is unhandled, just uncustomized.

Reading the tree: **constructs gate expressiveness, functions gate verbs**, and they interleave — e.g. `scan_enemies()` returns a list, so it requires lists. (Branching needs no place in this ordering: it ships at game start beside the start kit's own predicates — Q117.)

## Design Rules

1. **Every unlock changes what programs *can say*, immediately.** No "+5% damage" research. That lives in XP ([02-agents.md](../02-agents.md)) and hardware.
2. **The editor advertises the tree.** Locked syntax/functions are visible but greyed out in the editor with cost and prerequisites ([01-language.md](../01-language.md)). The player wants variables because they *felt* their absence, not because a tooltip said so.
3. **Enemies preview unlocks.** Ferals use constructs before you have them ([04-enemies.md](../04-enemies.md)) — Warden's `for`-loop patrol is an ad for Tier 5, readable once you've killed enough Wardens to decrypt it. The preview is earned like everything else.
4. **Data sources force breadth** — milestones span mining, exploring, combat, analysis, so a one-note strategy starves research (see Data rules in [03-resources.md](../03-resources.md)).
