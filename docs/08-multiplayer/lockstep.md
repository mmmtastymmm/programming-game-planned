*Part of [08-multiplayer](../08-multiplayer.md).*

# Model: Deterministic Lockstep

This game is unusually well-suited to lockstep:

- Player inputs are **rare and small** — deploy a program, queue a print, place a structure, research. No per-unit micro spam. Bandwidth is trivial.
- The sim (including every Pyrite VM step) is deterministic by construction ([07-architecture.md](../07-architecture.md)).
- Bot counts can grow large; lockstep cost is independent of entity count (unlike state sync).

```mermaid
sequenceDiagram
    participant P1 as Player 1
    participant R as Relay / Host
    participant P2 as Player 2

    Note over P1,P2: both sims at tick T, identical state
    P1->>R: Commands for tick T+D (deploy program X)
    P2->>R: Commands for tick T+D (none)
    R->>P1: agreed command set for T+D
    R->>P2: agreed command set for T+D
    Note over P1,P2: each sim applies same commands at T+D<br/>→ states remain identical
    P1->>R: state hash @ T+D
    P2->>R: state hash @ T+D
    R-->>R: hashes match? else DESYNC event
```

- **Input delay D**: ~3 ticks (300ms at 10 tps). Invisible here — commands are "deploy code," not "dodge left." This is why lockstep's classic weakness doesn't hurt us.
- **Topology**: client-hosted relay for v1 (one player hosts; relay only orders commands, doesn't simulate ahead of others). Dedicated relay later if needed.
- **Desync handling**: per-tick state hash exchange ([07-architecture.md](../07-architecture.md) phase 9, the snapshot hash). On mismatch: pause, dump divergent-state diff to log (dev), attempt host-state resync (prod).
- **Late join / reconnect**: host serializes full sim state + tick; joiner loads and enters lockstep. Same path as save/load — build once.
