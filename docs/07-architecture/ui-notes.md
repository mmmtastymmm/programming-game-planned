*Part of [07-architecture](../07-architecture.md).*

# UI Notes (editor & inspector)

- Code editor: egui (`bevy_egui`) to start — fastest path to a functional editor with gutter annotations (per-line cycle costs, live program counter). Custom polish later.
- Inspector = same widget pointed at any bot's `VmState` in read-only mode, **filtered through the viewer's decryption level** for foreign bots (player or Feral): revealed characters render, the rest is stable noise — except structural whitespace (line breaks, indentation), which always renders (Q125) — and the live program counter only steps over revealed lines. One widget, one masking pass — the transparency rules fall out of architecture.
- Codex ([04-enemies.md](../04-enemies.md)) is a `ProgramLibrary` view with diffing.
