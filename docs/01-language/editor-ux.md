*Part of [01-language](../01-language.md).*

# Editor & Player Experience

- In-game code editor with **per-line cycle-cost annotations** in the gutter.
- Live view: click any bot to watch its program counter step through lines in real time.
- Locked constructs appear in the editor greyed out with their unlock requirement — the editor *is* the tech-tree advertisement.
- Handler templates render as the full sandwich: forced prologue/epilogue lines shown locked (phantom lines, not in your source) around the editable window, with the window's remaining instruction budget and non-signal-safe functions greyed out inside it.
- **The implicit forever-loop is drawn, in color**: every program renders inside phantom `while True:` / loop-end lines **tinted the program's color** — a Green program visibly sits inside a green loop, a Red one in red. Same idiom as the handler sandwich: engine truth shown as locked lines you can't type on. It teaches the loop, brands the file with its printer, and marks exactly where a redeploy will land (the loop boundary).
- **Multi-window editing**: programs, modules, and individual handler windows each open as independent, movable, dockable editor windows. A hurt window and the module function it calls can sit side by side; the live view can dock next to the code it's stepping through.
- **The colony file viewer**: a tree of all the colony's code — one node per color (shown with its printer swatch, deployed-version status, and in PvP its enemy-decryption %), the module library, and each program's handler windows nested under it. Double-click opens a window. Handler windows are files like any other here — reachable, diffable, editable from the tree, with their caps and safe-sets enforced at deploy as always.
- Programs are validated at deploy time; using a locked construct is a parse error with a friendly "requires <unlock>" message.

