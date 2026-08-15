# Known Problems Register

Defects found in the design docs that are **not open questions** — nobody is
undecided about them. Each is a decision that was made and then contradicted, a
number that does not survive arithmetic against the constants it derives from,
or a ratified decision the implementation never caught up to. They live here
until fixed, then move to the **Fixed** log at the bottom with the commit that
closed them.

Numbering is stable — **append new problems, never renumber**. Open design
questions still go in [QUESTIONS.md](QUESTIONS.md); this file is for text that is
already wrong **or a ruling the code has not caught up to** — never for anything
still undecided.

**The third class closes differently, and shares its entry with a task.** An
implementation-lag entry (P37 is the first) is fixed by *code*, so it moves to
the Fixed log with the implementing commit rather than with a docs edit, and the
work itself is tracked in [TASKS.md](TASKS.md)'s *Decided-but-unbuilt* section.
Living in both files is the normal pipeline, not duplication: P1, P5 and P22 each
sit in the Fixed log with a task still open against their ruling, and a register
entry routinely *creates* the task that will close it — P37 created *Kind-constant
catch-up* in the same commit that opened it. The register owns the **gap**; the
task owns the **work**.

**⚠HASH in this register is a forward pointer**, not a claim about the fixing
commit: it marks an entry whose *subject* moves golden-replay hashes once someone
builds it. A fix here essentially never moves one — by the charter above this
file corrects text, and its commits touch docs only — so the marker exists so a
grep for hash-affecting work finds the ruling as well as the task.
[TASKS.md](TASKS.md)'s definition, "changes golden-replay hashes," is the one
that binds *there*, on work that ships code. Do not put the marker on an entry
whose subject is already implemented and already hashed.

Line references are as of the sweep that found them (2026-07-28, `git diff
@{upstream}...HEAD -- docs/` at commit `1f4ffb6`) and will drift as the docs
are edited — the quoted text is the reliable anchor.

**Re-anchored 2026-07-31** for the doc split: `01-language`, `02-agents`,
`03-resources` and `05-terrain` became doorway + directory, so every citation
into those four now names the *part file* that holds the quoted text. Line
numbers were dropped wherever the split invalidated them and kept only where
re-verified against the new file. No finding changed — this is a pointer fix.

**Re-anchored 2026-08-01** for the second doc split: `04-enemies`,
`06-progression`, `07-architecture`, `08-multiplayer` and `09-quirks` became
doorway + directory, so every citation into those five now names the part file
holding the quoted text, with line numbers re-verified against the new files.
No finding changed — pointer fix only. The same day's open-questions sweep
moved the answered worksheet bodies (Q111–Q123) out of `QUESTIONS.md` into
[history/questions-worksheets.md](history/questions-worksheets.md); citations
into those bodies now point there.

**Re-anchored 2026-08-02:** the fix rounds recorded above shifted their own
carriers after the 2026-08-01 pass — eleven Fixed-log line numbers had
drifted (five into `02-agents/xp-and-specialization.md`, two each into
`01-language/builtins.md` and `01-language/syntax-tiers.md`, one into
`07-architecture/world-state.md`, and `08-multiplayer/decided.md`'s Q86 line,
pushed down by the Q124/Q125 closes). Each number re-verified against the
current file. The quoted text remains the reliable anchor. *(A twelfth was
caught later the same day: P22's own citation into
`01-language/signals-and-logging.md`, shifted one line by the guard the P22
fix itself inserted — now :18.)*

**Re-anchored 2026-08-12:** a full-corpus audit re-resolved every `file.md:NN`
citation in this register against the current tree. Two were wrong, and both
belonged to *open* entries: P32's pass-assignment anchor landed on a blank line
(`:25` → `:26`, with the quoted sentence added), and P30's quoted anchor — the
recovery mechanism this file's header guarantees — no longer matched its
carrier, which has since been rewritten to name the gap in its own voice. In
the same pass **P33's scope was widened from one catalog row to the six that
share its defect**, so the ruling is taken against the class rather than the
instance. No finding changed and nothing opened or closed by the anchor pass
itself; the same audit's substantive findings are being ruled one at a time, and
the first of them opened and closed **P34** the same day (status block below).

**Dated status blocks below are point-in-time records — supersede, never
back-edit.** When the board changes, add a new dated block above the last one and
drop `(latest)` from its predecessor. Do not reopen a block to add an entry,
correct a count, or extend a sentence: an amended block stops describing any real
moment. The 2026-08-12 block was edited in place four times in one day, which is
why its entries read P36 → P37 → P35 → Lazy Evaluation → P34 and why a stale
"five open entries" count reached [QUESTIONS.md](QUESTIONS.md) and survived
there. This rule used to live *inside* the status blocks, where superseding them
carried it away — hence its being restated here, in text that stays.

**Status 2026-08-15 (latest): 42 opened, 35 fixed — seven open.** The two items
sitting in `docs/personal_problems.md`, a file referenced by nothing in the repo
and triaged into no register, are now **P41** and **P42** — both opened and
closed today. **P41**: the glossary ratified *Fabricator / Printer* as a dual
name, so neither spelling was wrong and both spread to twenty-one live-doc
mentions and eleven in `crates/`, while Pyrite only ever knew `printer`. Swept to
**Printer** on the owner's ruling; comments and test messages only, no identifier
touched. **P42**: Q127 adopted **allegiance** for a building's owner, colliding
with the glossary term for a nest's tarot rank 0–21 — which owns a file name, a
22-row table and a doorway invariant. The Arcana meaning wins; Q127's vocabulary
becomes **faction**, its substance untouched. The glossary gains a **Faction**
row separating faction / colony / team / color.

Three citations into
[03-resources/structures-and-start.md](03-resources/structures-and-start.md) were
re-pointed in the same pass: deleting that file's empty section earlier today
shifted every line below it by three, which silently moved P38's and P40's
anchors and Q128's onto the wrong rows. Re-verified against the current file.

**Status 2026-08-15 (later): 40 opened, 33 fixed — seven open; no entry moved.**
Two structural gaps the corpus audits had walked past, neither warranting a
number. The **01-language doorway** gained the kind-registry invariant P36 needed
and never added: kind constants are pre-bound globals, so the registry is part of
the load-time contract — two peers built from different lists disagree about
whether a program loads at all, which is divergence before the first tick. The
bullet also records the boundary P39 crossed, that the registry says what exists
and never what a query reaches. Separately, **two empty `##` sections** were
removed — `03-resources/structures-and-start.md` and
`05-terrain/map-generation.md` each carried a heading with no body, orphaned when
the split promoted the file titles and duplicated by the real sections below
them. No inbound anchor links to either.

**Status 2026-08-15 (earlier): 40 opened, 33 fixed — seven open; no entry moved.**
A convention round, closing the review item that the 08-12 block's four in-place
amendments raised. The supersede-never-back-edit rule is promoted out of the
status blocks into this header, into [QUESTIONS.md](QUESTIONS.md)'s header, and
into `CLAUDE.md` — the three places that outlive any one block. The 08-12 block
itself is left as written: its order is not chronological and is not being
repaired, because reordering it would be a fifth amendment. Only one purely
mechanical fix was applied to it — a 140-character prose line, spliced when
**P35** was inserted mid-sentence, re-wrapped to the file's width with no word
changed. (It was the sole plain-prose line over 110 characters in a file of 1275
lines at 85 or under; the other long lines are forced by markdown links.)

**Status 2026-08-14: 40 opened, 33 fixed — seven open.** **P38** joins
the needs-a-ruling class: Q89 ruled that a Depot's `faction` governs perception
and stopped there, while `deposit()` and `withdraw()` both accept any adjacent
depot whatever its faction, and the only statement of that policy anywhere in
the repository is a comment on the field itself. Like P29, it is a defect whose
fix is a ruling it does not own — **Q128**, opened the same day, takes the
general question of what relationship a building interaction requires, and P38
closes when docs/03 carries the answer. The 2026-08-12 block below is kept as
written rather than amended: it had already been edited in place four times,
which is how its "five open entries" count reached QUESTIONS.md wrong.

Also this day, **P39** opened and closed. The kind-constant registry claimed
every placement but `blight` is perception-gated — against P22's pool rule as
carried in ten passages across seven files — and had picked up Q99's
"perception-gated like a structure" as a second carrier of **P29**. Both struck:
the registry now cites the domain rules instead of restating a model it does not
own. Two stale `<hash pending>` placeholders were backfilled in the same change
(**P35** `33b1de8`, **P36** `a41ec41`), so every Fixed entry again names the
commit that closed it.

**P40** opened and closed the same day, out of P39's audit: P36's evidence bullet
claimed the Foundry recipe spells the material plural, and it spells it "Chip" in
all seven of its recipe positions. The sweep behind it found no drift but a
convention — **Chips** names the material, **a Chip** is one unit — so the split
is ratified in [03-resources/decided.md](03-resources/decided.md) rather than
swept, with the trap it carries (`chip` names nothing; the constant never
inflects) stated at the constant. P36's ruling stands and now carries an
amendment note recording what its evidence got wrong.

**P37 was corrected in place, not closed** — it stays open, and the register
totals are unchanged. Its arithmetic did not survive its own inputs (thirteen
structures minus five shipped is eight, split as 6 + 3 = 9) because it counted
`ally` as a sixth unbuilt structure. `ally` is a **bot** constant, and alliances
shipped with M13 and are hashed, so it is a *fourth* instance of the gap P37
exists to record rather than milestone lag — and TASKS.md had ruled it out of
scope on the ground that it does not exist. Both carriers fixed. The correction
also surfaced a decision the catch-up task now has to take: `find_kind`'s `enemy`
arm filters on faction alone, so `closest(enemy)` returns a declared ally, and
`World::allied` is consulted nowhere in `host.rs`.

**⚠HASH is now defined for this register** (header above). All four uses sat on
docs-only commits, which read as contradicting TASKS.md's "changes golden-replay
hashes" — but three of them (P3, P34, P36) were using the marker coherently as a
*forward pointer* to work that moves hashes when built, which is the only sense
available in a file whose charter is fixing text. That sense is written down
rather than stripped, since a grep for hash-affecting work is more useful finding
the ruling too. **P35 lost the marker**: its own resolution says the shipped VM
had already decided the rule and the spec was catching up, so nothing moves now
or later. Also corrected: no CI script reads the marker — it is reviewer-facing
only, so nothing was gated on the ambiguity.

**The register's charter now admits its third class.** P37's opening commit
updated the *Mechanical* class heading to cover "a ratified list the
implementation never caught up to" but left the file header saying this file is
for "text that is already wrong" — so the register contradicted itself about what
it accepts, one screen apart. The header now names implementation lag alongside
the other two classes, records that such an entry closes with the *implementing*
commit rather than a docs edit, and states how it relates to TASKS.md. The class
heading is renamed to *decided, then not carried through*, which covers both
un-propagated text and un-built rulings; nothing links to the old anchor. No
entry moved and no count changed. One thing worth recording against a
misreading: P37 did not duplicate a pre-existing task — `a41ec41` created
*Kind-constant catch-up* in the same commit that opened P37, which is the
pipeline working, not a second bookkeeping path.

**Status 2026-08-12: 37 opened, 31 fixed — P29–P33 remain open and need
rulings; P37 is open but needs only implementation.** **P36** closed the
kind-constant inventory (`blight` missing though shipped, `barricade` missing
though decided, `chip` for `chips`), and opened **P37** going the other way:
three structures ship with no constant to find them. P37 is the register's first
entry where the *design* is right and the *code* is behind — filed because
nothing else recorded it, and tracked by a task rather than a ruling. **P35** —
the blocking-burn rule never said whether a bank held from before the block
survives it — was opened and fixed the same day. It is the
round's odd one out: it was found by a *comprehension question about the
execution model*, not by any sweep, and it was invisible to grep precisely
because all five carriers agreed with each other and were incomplete in the same
place. The shipped VM had already decided it; the docs simply never said.
Also this round, the **Lazy Evaluation** quirk was cut (ruling in
[09-quirks/decided.md](09-quirks/decided.md)), which closes one of P33's six rows
and leaves five.

P34 was opened and fixed the same day: the Q56 granularity entry still
stored XP in deci-units, a month after Q111 replaced deci with centi-points. It
took a number rather than being swept silently because *decided text left behind*
is the class this register exists to prove recurs — and because the entry's
survival is instructive: it matched the shipped code (the centi migration is
queued, not built), an adjacent *Checked and cleared* entry had created an
exemption that read as covering it, and the contradiction sat six lines from its
own refutation in the same file.

**Status 2026-08-03: 33 opened, 28 fixed — P29–P33 are open and need
rulings.** P32 and P33 were opened by the full-corpus consistency audit the
same day: neither could be swept, because both ask which of two hash-affecting
readings is the design (the Combat-pass movement question, and what the Merge
Conflict quirk is actually allowed to modify).

**Status 2026-08-03 (earlier): 31 opened, 28 fixed — P29, P30 and P31 are open
and need rulings.** P31 was opened by the 2026-08-02 signal-safety deletion
(`1def1cc`): removing the loop ban made an unbounded window legal, and every
recovery path in the design is polite to mid-template bots, so the bot is
unrecoverable and — uniquely — silent about it.

**Status 2026-08-02: 30 opened, 28 fixed — P29 and P30 are open and
need rulings.** Four further high-effort audits followed the third, and their
findings were fixed in the same commit that found them rather than sitting
open as numbered entries: `8e17776` (6 — the last Q125 carrier in
07-architecture/ui-notes.md, eleven drifted Fixed-log anchors, two
false-premise tasks, the "(Q67 open)" marker, a dead intra-file link),
`31b5bb9` (5 — two doorway drifts in 07-architecture.md and 02-agents.md, the
Combat kill-bonus ratio, two stale TASKS.md markers), `2d4818c` (2 — the
02-agents doorway's pre-Q123 single-curve claim, P22's own off-by-one anchor),
`8aef987` (4 — TASKS.md state defects: a stale Q71 cross-reference, duplicated
status markers, an un-annotated Pump note, a verb-index milestone), and
`740539c` (10 — surviving Q111/Q115 carriers in 02-agents, two stale pointers
in 03-resources, and TASKS.md freshness). **The classes recur** — drifted
anchors appeared in three separate rounds — which is the argument for logging
them here rather than only in commit messages. The two findings that could not
be swept, because the docs do not contain the answer, are opened above as
**P29** (barricade query domain) and **P30** (Feral walks vs. P7).

**Status 2026-08-02 (earlier): still 28 opened, 28 fixed — a third audit corrected the
record, not the rulings.** The block below cites P22's close as `09c3e62`
(structure queries answer from faction knowledge); that close was twice
amended the next day — `6686866`, then `95c73c8` — to its final
**knowledge-pool** form (own colony state + granted allies', foreign
structures not query-reachable; see P22's amendment notes). No new problems
opened: the audit's ten findings were residual drift from already-recorded
closes, swept the same day — two stale carriers of P22's unguarded retreat
idiom (`05-terrain/tiles.md`, `05-terrain/map-generation.md`), the
code-visibility DECIDED paragraph never amended for Q124/Q125, a duplicated
clause in `01-language/types-and-env.md`, the Q109/Q110 rulings absent from
`history/questions-answered.md`, the stale `history/README.md` index
(Q123 → Q126), and two relative links broken by the verbatim moves into
`docs/history/`.

**Status 2026-08-01 (final): 28 opened, 28 fixed — the register is clear.**
A post-close xhigh audit (`c87ee66`) corrected ten closes: two spec gaps
(try_ pass assignment, the P22 ownership filter), two arithmetic errors
(P1's grade chain, P2's Hauling base), and stale carriers of P5, P7–P11,
P21, P22; amendments below record each. P1 — the bootstrap
deadlock — ruled and closed in `2c56fdf` (ruined Upgrade Station in the
start base). Earlier the same day the mechanical propagation batch — P8,
P12, P13, P15–P17, P19, P21, P23, P24, P26, P28 — closed in `93d6b25`.
The last six (P9–P11, P14, P18, P25) closed in `d5b561f`. P2
closed in `d90a428` (pacing table recomputed), P3 in `c1b26a7` (component
BFS + non-minting visible hold), P4 in `e913c27` (try_ covers the action,
never the argument), P22 in `09c3e62` (structure queries answer from
faction knowledge), P6+P20 in `3e21e89` (both linear perks converted to
the bounded hyperbolic), P7 in `84e1e68` (starter walks try_, tail
wanders), P5 in `9921848` (hyperbolic grouping/rounding/liveness are spec), P27 in `a2a81ff` (occupancy layer).

**Status 2026-08-01: 28 problems opened (P1–P28), 0 fixed.** P15–P18 were
found by the reviews of the 04–09 doc split; P19–P28 by the same day's
full-corpus consistency audit (post-split, commit `406c837`). Appended below —
P20/P22/P27 under *Needs a ruling*, the rest under *Mechanical*.

**Status 2026-07-28: 14 problems opened (P1–P14), 0 fixed.** All come from one
max-effort doc-coherence review of the Q111–Q123 sweep (76 agents, 84 candidates
verified, 8 refuted) — fifteen verified findings, two of which are the two
halves of P7. They collapse to two failure modes, and both are process problems
rather than authoring mistakes:

1. **A ruling landed in QUESTIONS.md plus one or two docs while the *owning*
   doc kept specifying the superseded mechanism.** Under this repo's own
   convention the owning doc's *Decided* section is the normative record an
   implementer builds from — so for P7 the stalest text in the repo sits in the
   most authoritative place.
2. **A new tuning number was never cross-checked against the constants it
   derives from.** P2 is the flagship: one substituted rate inverts the
   conclusion the ruling that introduced it was written to establish.

The three that change actual game behavior rather than doc clarity are **P1**
(the colony cannot bootstrap at all), **P2** (the XP two-tier is backwards) and
**P3** (a hash-affecting rule specified two incompatible ways).

---

## Needs a ruling

These cannot be swept mechanically — the docs do not contain the answer.

**P29 — the barricade query domain is specified two incompatible ways in one
Decided file.** [05-terrain/decided.md](05-terrain/decided.md) (the Q99
barricade bullet vs. the P22 structure-pool bullet five lines below);
[TASKS.md](TASKS.md) (*Decided-but-unbuilt*: "Barricade HP (Q99)" vs.
"Structure-pool query domain (P22)").

Q99 gives barricades a `barricade` kind constant expressly so an assault force
can find and shoot through a rival's wall, and says finding one "takes eyes" —
i.e. perception suffices. P22's final form removes foreign structures from the
query domain **entirely** — no perception path, no selector. A rival's wall is
therefore both findable (Q99) and unfindable (P22). Both readings are
hash-affecting, so two implementers ship divergent `closest(barricade)` domains
and desync; meanwhile the documented siege idiom faults every loop.
Needs one ruling: are enemy barricades (and any other attackable foreign
placement) a carve-out from P22's own-pool rule, or does breaching become a
pure adjacency/terrain interaction with no query surface?

*(The ruling is being taken as **Q127**, opened 2026-08-02 — the question
grew past this entry into building ownership and the remembered-building
query surface generally. P29 closes when Q127 is answered; the substance
lives in QUESTIONS.md, not here.)*

*(A second carrier appeared and was removed on 2026-08-14: P36's kind-constant
entry copied Q99's "perception-gated like a structure" into
[01-language/types-and-env.md](01-language/types-and-env.md), and **P39** struck
it, so the contradiction is again confined to one file. The phrase travels
easily — anything restating Q99's gating outside 05-terrain/decided.md should be
checked against this entry before it lands.)*

**P32 — the three Combat-pass verbs all move the bot, which is the one thing
the move→combat split exists to prevent.**
[07-architecture/tick-model.md:26](07-architecture/tick-model.md) (the
verb-by-verb pass assignment — "**Pass assignment is part of the spec, not an
implementation detail**") vs. the same entry's rationale at :24 ("every mover
finishes stepping before the first swing range-checks").

Pass assignment is declared hash-affecting spec — "it must not be invented per
implementation" — and it puts `attack`, `guard` and `escort` in **Combat**,
while **Move** lists only `move_to`, `wander`, `explore`, in-flight `Move`,
bump-freeze replans and the engine walks. But all three Combat verbs move:
`attack(contact)` on a heard-only contact "closes to engage, resolving on
sight" ([01-language/decided.md](01-language/decided.md), Q74), and
`guard`/`escort` hold near and follow their entity. The split's stated purpose
is that "every mover finishes stepping before the first swing range-checks",
which a Combat-pass step defeats — and the movement those verbs perform is
assigned to no pass at all.
Needs one ruling: does the closing/following **step** ride the Move pass while
only the swing resolves in Combat (preserving the invariant, at the cost of one
verb spanning two passes), or do Combat verbs move within Combat and the
invariant get restated as "movers *whose action is a move verb* finish first"?
Both are hash-affecting; picking by implementation is exactly what the entry
forbids.

**P33 — five catalog quirks modify something that is neither a stat-sheet row nor
a cost-table overlay, against quirk design rule 6.** *(Opened on one row, widened
to six, now five — Lazy Evaluation was cut on 2026-08-12.)* Flagship: the Merge Conflict
quirk modifies a literal inside replaceable factory Pyrite.
[09-quirks/catalog.md:53](09-quirks/catalog.md) (the
Merge Conflict row at `:53`: "the bump factory window's built-in `wait` runs +50%
longer") vs. [09-quirks/design-rules.md:10](09-quirks/design-rules.md) rule 6 and
[02-agents/stat-sheet.md](02-agents/stat-sheet.md)'s canonicity rule.

Rule 6 requires every quirk to modify a stat-sheet row or lay a per-bot cost
overlay; this does neither. The bump stun is factory contents —
`wait(35)` in replaceable Pyrite ([01-language/decided.md](01-language/decided.md))
— not hardware and not a stat, so no row declares its unit scale, pipeline
position or rounding, and +50% of 35 (52.5) can round either way on two peers.
Implemented literally, by editing the window's argument, it is worse: two bots
of the same color would run different program source, breaking the byte-exact
per-color artifact hashing that version identity and the decryption reveal mask
both rest on. Same defect class as the fixed P14 (XP gain) and P25 (Boot
ritual), both of which were closed by *adding the missing stat row*.
Needs one ruling: give the bump stun a stat-sheet row and re-express the quirk
against it (the P25 precedent — but note the player can delete the factory
`wait` entirely, so the row must define what the quirk scales when the window is
empty); re-express it as a cost-table overlay; or cut the quirk.

*(Scope widened 2026-08-12 — the entry was opened on one row, but a rule-6 sweep
of the whole catalog finds six. The ruling has to be applied catalog-wide, or it
fixes one row and leaves the rest; each is hash-affecting, and none names a row.
**One of the six, Lazy Evaluation, has since been ruled and cut** — it carried a
second, larger defect and did not wait on this entry — so **five remain open**
below.)*

  - **Merge Conflict** (`:53`) — the flagship above.
  - ~~**Lazy Evaluation**~~ — **RESOLVED 2026-08-12: the quirk is CUT**
    ([09-quirks/decided.md](09-quirks/decided.md)). It banked the budget while
    blocked, against three canonical passages that state the opposite without
    qualification. Ruled separately from the rest of this entry because the defect
    was larger than rule 6: since **actions** block (Q100), the row was not the
    Tier 7 listening-post perk it was billed as but a general one — banking during
    every walk and every swing, making thinking free between actions and
    reintroducing the Coprocessor Q100 retired. Rescoping it to channel blocks was
    declined; the rule is now a cross-part invariant in the
    [01-language doorway](01-language.md), which is where its absence let the
    contradiction in.
  - **Simulated Annealing** (`:26`) — "may sidestep to neighbors that lose up to
    1 tile of ground toward the goal" relaxes the ratified sidestep rule in
    [02-agents/decided.md:18](02-agents/decided.md) ("among free neighbors that
    **lose no ground** toward its goal"). A movement rule, not a row.
  - **Off-by-One** (`:49`) — "every Kth `move_to()` stops one tile short of the
    target". No row, and it brushes design rule 1 as well ("never … moves the bot
    on its own" — here it *stops* the bot short of the instruction it was given).
  - **Eventual Consistency** (`:70`) — the cycle half is a clean cost overlay,
    but "returns data that is **one additional tick stale**" modifies perception
    latency, which is spec in [07-architecture/tick-model.md:28](07-architecture/tick-model.md),
    not a sheet row.
  - **Thermal Runaway** (`:73`) — "+20% move speed" is a row; "its wreck's blast
    damage is doubled" is not. Blast is *derived* from the wreck's max HP
    ([02-agents/damage-faults-death.md:44](02-agents/damage-faults-death.md);
    50% of max HP in [TASKS.md](TASKS.md) M10), so doubling the blast without
    doubling max HP is an unrowed multiplier on a derived quantity.

**P31 — an unbounded handler window strands the bot in a state nothing can
recover, and it is the one failure in the game with no wreck and no crash
dump.** [01-language/faults-and-handlers.md](01-language/faults-and-handlers.md)
(the 2026-08-02 window redesign) vs.
[01-language/program-colors.md](01-language/program-colors.md) (polite recall,
the loop-boundary hot-swap) and
[07-architecture/vm.md](07-architecture/vm.md) (the pad pull skips mid-template
bots).

Deleting signal-safety made `on boot: while True: wait(1)` legal. A bot looping
in a window is permanently mid-template, and every recovery path in the design
is explicitly *polite* to mid-template bots: recall retries each tick and never
lands, the over-capacity scrap valve re-selects past it, the Upgrade-Station
pad pull skips it, and the hot-swap only lands at a loop boundary the bot never
reaches — so not even a corrected redeploy can reach it. The double-handle rule
does not save it either: a bot looping somewhere safe receives no second signal,
so nothing forces the abort that would at least produce a wreck. Every other
failure in this game is visible and diagnosable; this one is a bot that silently
stops, forever, with an intact colony around it.

*The root cause is the unbounded politeness, not the loop.* Recall **is** a
signal and double-handle **does** apply to it (`decided.md`: "Recall is an
interrupt context — double-handle applies all the way home"), so the obvious
expectation — a recall landing on a stuck bot aborts it, producing a wreck —
is the design's own rule. What blocks it is Q85's dispatch split: recalls the
*player didn't time* (deploy-triggered drops/claims, over-capacity scrap)
"never enter mid-template", while player-fired triggers (rule edits, the check
interval) "dispatch like signals — your clock, your risk". Politeness has no
deadline, so a bot that never leaves its template is never dispatched to.

*The exposure is not uniform, and `on boot:` is the worst case.* The M9 review
round further defers **booting and pad-sitting** bots to the polite queue even
in signal mode ("engine states aren't the player's clock — only mid-TEMPLATE
landings keep the double-handle gamble", [TASKS.md](TASKS.md) M9). So a
boot-window loop is exempt from *both* dispatch modes and has **no escape at
all**, while a loop in `error`/`hurt`/`bump`/`bumped` retains one narrow,
accidental escape: a player-fired rule edit or the check interval can land and
abort it — but only if the allocation happens to want to move that bot, so a
correctly-allocated bot is never dispatched to and stays stuck regardless.

*A related gap this exposed, worth a sentence either way.*
[01-language/decided.md](01-language/decided.md) states "**Boot participates in
double-handle** — any signal mid-boot aborts the bot back into a wreck", while
the M9 rule means recall never *becomes* such a signal for a booting bot. The
two are reconcilable — the signal is never sent, so the abort rule is vacuous
rather than violated — but nothing says so, and a reader of the decided bullet
would expect recall-during-boot to abort.

*A fourth candidate considered and set aside as insufficient on its own
(2026-08-03): **delete politeness entirely** — every signal, recall included,
interrupts everything all the time.* Two real arguments for it. It restores a
rule the design already states absolutely —
[01-language/faults-and-handlers.md](01-language/faults-and-handlers.md): "There
is no safe phase in the sandwich" — of which politeness is the only carve-out.
And it makes the double-handle pricing *actually* load-bearing: the 2026-08-02
window redesign made that rule the sole price of handler length, then left the
most common interrupt source exempt from it, so handler length is currently
priced by a mechanism that politely declines to fire. It also needs no tuning
constant, unlike the deadline. **But it does not close this entry**, for the
reason the deadline candidate also has to answer: politeness only matters where
a recall is *dispatched*, and the allocation re-colors only bots whose
assignment changed — a bot already at the right printer is never dispatched to
at all ([TASKS.md](TASKS.md) M9 *Dispatch rules*: "a same-color re-target
cancels in place", and an already-walking re-color has its destination updated
"no re-signal"). The P31 bot in a balanced colony has no polite
recall being deferred; it has **no recall**. Removing politeness therefore fixes
only the subset where fleet arithmetic independently wants to move the stuck
bot, which makes recovery depend on unrelated bookkeeping. Its cost is also
correlated in the wrong direction: rebalancing fires hardest after casualties,
exactly when survivors are sitting in `hurt` and `bumped` windows, so a bad
fight would be followed by an allocation pass killing a second wave for having
flinched — the "triggers the player didn't time" line Q85 drew. That may still
be the game you want (it is pressure toward short handlers, delivered by play
rather than by a compiler), but it should be ruled as a deliberate lethality
increase in its own right, not as this entry's fix.

*Consequence for the candidate list:* only a **template-side** rule catches a
bot that nothing is dispatching to, so the overtime rule closes this entry
independent of fleet state. It also makes politeness harmless afterwards — once
no template can run forever, a polite recall can only ever be deferred a bounded
time — which leaves "keep or delete politeness" a free, separable choice.

Needs one ruling. Three candidates, none of them swept in: accept it as the
player's problem; **give politeness a deadline** — an engine-fired recall stays
polite for N ticks, then lands anyway, so a normal two-line hurt handler is
never touched while a looping bot aborts into a diagnosable wreck and frees its
fleet slot (one engine rule, no language change, and it targets the mechanism
rather than the symptom); or add a **runtime overtime rule** where a template
running past N ticks aborts — noting that
[01-language/cycle-costs.md](01-language/cycle-costs.md) records the deleted
caps as having *replaced* an earlier "grace-window/overtime tax", so this third
option is a deliberate revival. Reserving `on boot:` was considered and does not
close it: `error`, `hurt`, `bump` and `bumped` windows have the same property,
and boot's window is the documented home of the run-once dotfile idiom
([01-language/types-and-env.md](01-language/types-and-env.md)), which the main
program cannot replace without re-charging the config every loop-around.

**P30 — the shipped Feral walks keep the bare blocking `move_to` that P7 ruled
lethal, on a waiver that cites a different fault.**
[04-enemies/archetypes.md:9](04-enemies/archetypes.md) ("The Drone and Stinger
keep their faulting `move_to`/`attack` as shipped, and the Q108
`move_to`-before-swing guard is their lesson — but that guard covers the
*non-adjacent swing*, not the *no-path* fault P7 ruled lethal for the Tier-0
starter, so whether these walks should be `try_` is open (P30)").

P7 made the Tier-0 starter's walks `try_` because a no-path fault every loop,
at Q109's `fault_damage` 2 against 40 base HP, kills a bot in ~8 seconds. The
Feral waiver rests on Q108's guard, which addresses a *non-adjacent swing* —
a different fault entirely. `exists(enemy)` is true for any perceived enemy,
including one across water or behind a demolished bridge, so a Drone or
Stinger that sights an unreachable target self-destructs unattended and nests
near water depopulate themselves. Q108's own principle ("shipped sources must
not crash-loop") points the other way from the waiver built on it.
Needs one ruling: do the attacker archetypes take `try_move_to` (⚠HASH — Feral
program text is hashed into the program library), or is unreachable-target
self-destruction intended Feral behavior that the waiver should state
positively instead of deriving from Q108?

*(Re-anchored 2026-08-12: the quoted sentence had been rewritten and no longer
grepped. The carrier now states the gap in its own voice and cites P30 rather
than asserting the waiver, so what remains open here is the ruling alone, not an
unmarked contradiction.)*

**P38 — Q89's depot ruling governs perception; the sim also enforces an access
rule that no design doc states.**
[03-resources/decided.md](03-resources/decided.md) (the Q89 depot bullet) and
[03-resources/structures-and-start.md:21](03-resources/structures-and-start.md)
(the Depot catalog row) vs. `crates/sim/src/world.rs:216` (the `Depot.faction`
doc comment), `crates/sim/src/actions.rs:402` and `crates/sim/src/host.rs:606`.

Q89 gave the Depot a real `faction` field and ruled that it **sees/hears for
its owner** — "One rule across the sim's perception, reachability checks, and
the fog view." It says nothing about who may *use* one. The sim has a rule
anyway: `deposit()` and `withdraw()` each accept any adjacent depot whatever
its faction, while the structure arms of both verbs filter
`st.faction == faction` — production private, drop-off public. The only
statement of that policy anywhere in the repository is the comment on the field
itself: "Haul deposits/withdrawals stay open to anyone standing on it —
ownership governs perception, not access." A design rule is living in a code
comment, and it asserts a scope Q89 did not grant it.

The consequence is not a desync — the behaviour ships one way on every peer —
it is that the corpus cannot be used to check the implementation. The Depot's
catalog row reads "Cargo drop-off, storage."; nothing in docs/03 or docs/08
confirms or contradicts that a rival's depot works as a forward logistics base,
which is a strategically load-bearing rule a player has no way to learn. The
same comment also names "withdrawals" as an open interaction, though no
`Withdraw` action exists — `withdraw` is a host builtin — so the comment is the
sole carrier of a rule about a code path it does not sit on.

*(The ruling is being taken as **Q128**, opened 2026-08-14 — the question grew
past the depot into what relationship any building interaction requires. P38
closes when Q128 is answered and docs/03 states the access rule, whichever way
it goes; the substance lives in QUESTIONS.md, not here.)*

---

## Mechanical — decided, then not carried through

These need no ruling; the decision exists and was not carried through — usually
text that was never propagated, occasionally (P37) a ratified list the
implementation never caught up to.

*(Cleared 2026-08-01; **P34** and **P36** joined this class on 2026-08-12 and
closed the same day — see the Fixed log. **P37** is open below.)*

**P37 — three shipped structures and one shipped bot relationship have no kind
constant, so the registry is narrower than the inventory that owns it. OPEN
(tracked, not a ruling).**
[01-language/types-and-env.md:15](01-language/types-and-env.md) (the Structures
line) and :16 (the bots line) vs. `crates/sim/src/host.rs` (`KINDS`); task in
[TASKS.md](TASKS.md) (*Kind-constant catch-up*).

The ratified inventory lists thirteen structure constants; `KINDS` ships five
(`depot`, `smelter`, `foundry`, `archive`, `printer`). Five of the eight missing
structures — `pump`, `repair_bay`, `sentry`, `lantern`, `request_box` — are
ordinary milestone lag: the *thing* does not exist in the sim yet either
(`RepairBay` has zero hits in `crates/sim/src`), so there is nothing to find and
nothing to fix until those milestones land.

The remaining three are the actual gap: **`generator`, `geothermal` and
`upgrade_station` all have shipped structures** — `StructureKind::{Generator,
GeothermalTap, UpgradeStation}` are in `world.rs` and in `StructureKind::ALL` —
and no way for a program to query them. A bot cannot route itself to the Upgrade
Station it must "physically walk to" ([02-agents/decided.md](02-agents/decided.md),
the compute-buying ruling), which makes the documented upgrade loop unwritable in
Pyrite today. This is the inverse of the usual direction: the design is ratified
and the implementation is behind, with nothing recording it — the only
kind-constant task in the file was the Q127-blocked barricade one.

**`ally` is a fourth gap, and it is not a structure** (corrected 2026-08-14). The
inventory lists it under **bots** beside `enemy`
([types-and-env.md:16](01-language/types-and-env.md)), and alliances shipped with
M13: `World.alliances` is a hashed `BTreeSet<(u8, u8)>` (`world.rs:1274`),
`Command::SetAlliance` applies at `sim.rs:1470`, the relay authorizes it at
`lockstep.rs:200`, `World::allied` answers it at `world.rs:1865`, perception pools
on it at `perception.rs:131`, and `sim.rs:3025` folds it into the state hash. The
thing exists, is queryable in principle, and has no constant — the same defect as
the three structures, filed originally as milestone lag on the mistaken reading
that it named a building.

*(Arithmetic corrected in the same pass. The entry read "Six of the eight missing
names … plus `ally`", splitting eight into 6 + 3 = 9. Thirteen structures minus
five shipped is eight: five milestone-lag plus three real, with `ally` a separate
bot constant and a fourth gap. [TASKS.md](TASKS.md) carried the same miscount and
the same misfiling, and ruled `ally` out of scope on the ground that "those
structures don't exist in the sim yet".)*

Two decisions ride along. The doc's constant is **`geothermal`** while the code's
structure name is **`geothermal_tap`** — whichever wins, both must say it. And
`ally` cannot be specified without settling what `enemy` means beside it:
`find_kind`'s `enemy` arm filters on `b.data.faction == faction` alone
(`host.rs:223`), so **a declared ally is currently returned by `closest(enemy)`**,
and `World::allied` is never consulted anywhere in `host.rs`. Q91 ruled that
`guard()`/`escort()` auto-fire spares allies precisely to prevent accidental
friendly fire while explicit `attack()` stays legal — but that is a rule about
harm, and nothing rules the *query*. Whether `enemy` excludes declared allies, and
whether `ally` includes one's own colony (`allied()` counts a faction as its own
ally), are the two calls the implementation needs; either becomes a question if it
turns out to be contentious.

---

## Checked and cleared

Raised during the same review and **refuted** on verification — recorded so they
are not re-raised:

- **`closest_minable`/`exists_minable` leak live state through fog.** The
  predicate's scoping is consistent with docs/05's live-only remaining amounts.
- **`try_mine()` has no tie-break among in-range nodes.** Determinism is covered
  by the existing entity-ID rule.
- **Q120's "fails its own range check" implies a fault and an HP chip.** The
  same sentence disclaims it — "no fault, no HP chip."
- **Newly hash-affecting behavior in TASKS.md carries no ⚠HASH marker.**
  Markers are present where the convention requires them.
- **"Dense to grade 5" contradicts levels past 5 being pure score.** Both are
  true of different things.
- **The XP-core task mixes centi and deci units.** The deci figure was a stated
  conversion, not a storage claim — correct as far as it went. *(Retired
  2026-08-12 by **P34**: the exemption this entry created was read as covering
  the question, and the real storage claim sat one file over in
  `02-agents/decided.md`. The conversion has since been restated in centi, so no
  deci figure survives in the XP path and nothing here needs an exemption.)*
- **The tool licence's "or its total level" branch is inert all session.** The
  floored mean does reach useful values within a match.

---

## Fixed

*(entries move here with the fixing commit's hash when they close)*

**P1 — the Upgrade Station is priced in a material only the Upgrade Station can
unlock. FIXED (`2c56fdf`).**
[03-resources/structures-and-start.md:29](03-resources/structures-and-start.md) (the Station's price),
[03-resources/decided.md:16](03-resources/decided.md) ("The bootstrap works"),
[03-resources/harvest-tiers.md](03-resources/harvest-tiers.md) (the drill ladder),
[06-progression/upgrade-station.md:34](06-progression/upgrade-station.md)

The Station costs **10 Steel, 5 Chips, 3 Wire**. Chips are 1 Silver + 2 Crystal
+ 1 Wire, and Crystal is resource tier 4. A fresh print carries the free grade-1
drill, which reaches tiers 0–1 (Wood/Stone/Sand/Iron/Coal) only. Grade 2 — the
sole route to Copper/Tin, hence Bronze, hence the Foundry (25 Steel + 10 Bronze)
that makes Chips in the first place — is purchasable **only at an Upgrade
Station**, and no doc grants a pre-built one. The colony cannot build the
structure that sells the upgrade it needs to build the structure. It is hard
capped at Iron/Coal forever and no tool of any grade is ever buyable, while
"The bootstrap works (Q72)" at `:227` asserts the opposite **on the same page**.

The old formulation survived this because tools also had a Fabricator path;
Q105 folded tool-making into the one pad flow and Q118 narrowed the ladder rule
to bind **on the drill alone**, so the rule as written no longer catches the
case where the *seller itself* is priced above the ladder it sells.

A fix must do one of: grant a pre-built Station in the starting kit, reprice the
Station below tier 2, unlock drill grade 2 off-Station, or re-widen the ladder
rule to bind on structures that sell tools. **Whichever is chosen, the ladder
rule at `:227` needs restating so it catches this class, not just this
instance.**

*(Resolution: a **ruined Upgrade Station** in the start base, repairable for
tier-0/1 materials — the Red-Fabricator pattern — plus the seller-side ladder
corollary in [harvest-tiers.md](03-resources/harvest-tiers.md).)*

*(Amended: the ruling text's bootstrap chain misstated Crystal as drill-grade-2
reachable (it is tier 4 — grade 4); corrected in all three carriers, `c87ee66`.)*

**P2 — Mining's `curve_base` is derived from a rate 8× the docs' own mine yield.
FIXED (`d90a428`).**
[02-agents/xp-and-specialization.md:84](02-agents/xp-and-specialization.md) (the pacing table),
[03-resources/the-tree.md](03-resources/the-tree.md) (mine yield),
[history/questions-answered.md](history/questions-answered.md) (Q122/Q123)

The Q123 pacing table reads `| Mining | ~80 /tick | 32,000 |`. But
[03-resources.md](03-resources.md)'s tuning manifest fixes mine yield at **2
units/swing**, [02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md) fixes one `mine()` swing at **~20
ticks**, and Mining income is 1 XP (100 centi) per unit. A bot swinging nonstop
earns **200 centi / 20 ticks = 10 centi/tick**, not ~80.
([history/questions-worksheets.md:510](history/questions-worksheets.md)
repeats the same unchecked assumption in prose.)

With `curve_base` 32,000, Mining L5 costs 15 × 32,000 = 480,000 centi = **48,000
ticks ≈ 80 minutes**, against the stated 10-minute job-track target and the
50-minute ambient target. So an idle bot that never mines reaches Age or
Processing L5 **before** a dedicated miner reaches Mining L5: seniority beats
specialisation, which is precisely the failure Q123 exists to fix. Knock-on:
drill grade 2 (Mining L2 ≈ 16 minutes of *uninterrupted* swinging, far longer
once hauling is counted) gates Copper/Tin well past
[06-progression/pacing.md:11](06-progression/pacing.md)'s 15–30 minute
midgame beat.

`curve_base = dedicated_rate × target_ticks_to_L5 / 15` is sound; the
substituted rate is not. **Every job-track row in the table is one substitution
of a rate that was never checked against `costs.ron`'s action times** — the
whole table needs recomputing, not just Mining's row.

*(Resolution: Mining recomputed to 10 centi/tick → `curve_base` 4,000; the
other four job rows verified against their inputs (Hauling and Building
derive; Scouting and Combat annotated as duty-cycle placeholders) and a
derivation paragraph added so the table is recomputed, never re-guessed.)*

*(Amended twice: 600 → 560 (`c87ee66`) still baked display rounding; the exact
derivation (10/7 centi/tick × 400) gives **571**, landed in `6686866`.)*

**P3 — Q120 both mandates and forbids the same silent hold. FIXED (`c1b26a7`). ⚠HASH**
[03-resources/decided.md:8](03-resources/decided.md) ("HOLDS — silently") and
[03-resources/decided.md:10](03-resources/decided.md) ("never hold"); also
[history/questions-answered.md](history/questions-answered.md) (Q120)

Within one *Decided* entry: line 219 says that when the displacement BFS
exhausts, the completing build **"HOLDS — silently"** (re-parks and retries next
tick, no progress, no XP, no fault); line 221 says the build must **"never
hold"**, and that holding "was tried during M16 and was wrong."

The two readings produce different sim behavior — an infinite silent stall
versus whatever the never-hold branch does (fault, delete, or force-complete) —
and every alternative is hash-affecting, so **two implementations of the same
spec desync in lockstep multiplayer.**

[history/questions-worksheets.md:417](history/questions-worksheets.md)–`:427` carries only the unconditional "never hold" version and argues
the case cannot arise ("a colony's fleet cap sits far below the map's tile
count, so a legal state always has a free tile somewhere"), while
03-resources/decided.md explicitly rejects that argument ("no tile count argument
covers it"). They also disagree on the **BFS domain** — whole map versus the
build site's passable connected component — which is what decides whether a bot
sealed in a pocket by Mountain/Water/barricades is reachable at all. Both the
exception's existence and the search domain need one answer.

*(Resolution: component-scoped BFS ratified; exhaustion holds, non-minting
and UI-visible — the one legal stall. The "never hold" bullet now forbids
minting/faulting stalls specifically. The history log keeps the superseded
whole-map wording as a closed record.)*

**P4 — `try_*` verbs type-faulting on `Result` re-creates the double-handle the
amendment was written to remove. FIXED (`e913c27`).**
[01-language/builtins.md](01-language/builtins.md) (the `try_*` rows and signal-safe flags),
[01-language/types-and-env.md](01-language/types-and-env.md) (`Result`)

[history/questions-worksheets.md:220](history/questions-worksheets.md)–`:230` deleted the old unwrap rule because it left
`try_move_to(try_receive("orders"))` undefined, and "one reading makes that line
a fault inside a running handler, i.e. a double-handle that wrecks the wounded
bot it was meant to save." The replacement makes that line an **always**-fault
instead of a sometimes-fault. Both operands are signal-safe
([01-language/builtins.md](01-language/builtins.md)), so the idiom is legal inside
`on hurt:` and the fault lands in the handler.

Worse, `closest` and `closest_minable` return `Result` (see [01-language/builtins.md](01-language/builtins.md)), so the
natural spelling of "the fault-free walk" — `try_move_to(closest(depot))`,
**verbatim the code [history/questions-worksheets.md:264](history/questions-worksheets.md) shipped one amendment earlier** — is a
runtime fault. Nothing specifies a deploy-time type check; deploy validates only
program memory and variable slots ([02-agents/decided.md](02-agents/decided.md)). A
hurt-handler retreat written the obvious way turns every hurt signal into an
abort, i.e. the rescue-denial path.

A fix must pick one: `try_*` accepts and propagates `Result`/`Option`, or the
type error is caught at deploy (which needs the deploy validator's scope
widened), or `try_*` loses its signal-safe status (which costs more than it
saves).

*(Resolution — ruled the other way: `try_` covers the action, never the
argument. try_* verbs take concrete arguments; Result/Option arguments are
ordinary type faults, resolved before the verb by guard-then-act or match.
The contract is now stated in builtins.md and types-and-env.md; the
composition idiom is defined by exclusion rather than absorbed.)*

*(Note 2026-08-02: this entry's reasoning leans on signal-safety — "both
operands are signal-safe", the rejected "`try_*` loses its signal-safe status"
option — and that concept was deleted the same day. The reasoning is a closed
record; the ruling is unaffected, and the deleted flag only widens where the
idiom is legal, never where it faults.)*

**P5 — the bounded perk truncates to zero on integer stats. FIXED (`9921848`).**
[02-agents/xp-and-specialization.md:33](02-agents/xp-and-specialization.md) (the formula),
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) ("flat-only stats stay whole")

Q121's `bonus = max_bonus × level / (level + K)` is applied to sensor range and
max HP, which [02-agents/stat-sheet.md](02-agents/stat-sheet.md) keeps as whole integers
("Flat-only stats (HP, slots, sensor tiles) stay whole" — sensor range has no
`unit_scale`). With a plausible `max_bonus` of 3 tiles and K of 10, integer
division gives 3×1/11 = 0, 3×2/12 = 0, 3×3/13 = 0, 3×4/14 = 0 — **a bot that has
ground Scouting to level 4 sees exactly as far as a fresh print**, with no UI
signal that the perk exists. This contradicts the perk table's "sensor range
(bounded)" entry and `:167`'s "This is why every level still matters."

Two further gaps in the same formula: the doc promises "half of `max_bonus` at
level K" and 3×10/20 = 1, not 1.5, so odd `max_bonus` values silently lose their
claimed midpoint; and the **evaluation order is unstated** —
`max_bonus * (level / (level + K))` is 0 at every level forever, and nothing in
the spec rules that grouping out. A deterministic sim cannot leave that
ambiguous.

*(Resolution: grouping mandated — `(max_bonus × level) / (level + K)`, floor
division; bounds restated honestly (⌊max_bonus/2⌋ at K, strictly below
max_bonus forever); centi-unit progress display plus an xp.ron load assert
that every perk grants ≥ 1 unit by its track's L5.)*

*(Amended: the Hiding/Flinch stat-sheet rows still wrote the unparenthesized
grouping; swept in `c87ee66`.)*

**P6 — the Flinch perk saturates to zero, deleting the forced prologue
outright. FIXED (`3e21e89`).**
[02-agents/xp-and-specialization.md:66](02-agents/xp-and-specialization.md) (the Flinch row),
[09-quirks/acquired-quirks.md:8](09-quirks/acquired-quirks.md); Q121 in [history/questions-answered.md](history/questions-answered.md)

Q121 ratified Flinch's −10%/level as "self-saturating," but it saturates **at
zero**: "floors at L10" means a bot that has endured enough hostile flinches has
flinch duration 0. [02-agents/damage-faults-death.md](02-agents/damage-faults-death.md)'s "forced prologue on most
signals — time spent locked and vulnerable" then stops existing for veterans,
removing the vulnerability window the entire double-handle and rescue economy is
priced against. This is the one surviving linear perk Q121 declined to convert
to the bounded hyperbolic; converting it, or flooring it at a nonzero fraction,
are the two fixes.

*(Resolution: converted to Q121's bounded hyperbolic with `max_cut` below
100% — the prologue shortens, never vanishes. Ruled together with P20.)*

**P7 — the shipped Tier-0 starter faults to death on unreachable ore, and does
nothing at all when no ore is minable. FIXED (`84e1e68`).**
[01-language/syntax-tiers.md](01-language/syntax-tiers.md) (the shipped starter),
[01-language/builtins.md](01-language/builtins.md) (`move_to`'s no-path fault)

Two defects in one program, both introduced by Q117's rewrite:

  - **No reachability guard.** The starter guards drill grade and ore remaining,
    then unwraps into the **faulting** `move_to`. An Iron seam on the far bank
    of a river (water is impassable; sight is not blocked by it) makes
    `exists_minable(ore)` True, `closest_minable(ore)` return it, `.expect()`
    unwrap Ok, and `move_to` hit "the normal no-path fault" (see [01-language/builtins.md](01-language/builtins.md)). Nothing in
    the loop ever observes the node as unreachable, so the guard stays True and
    the program faults **every iteration** — 2 HP a fault, a 40 HP chassis dead
    in ~20, and every bot on the shipped program does it at the same seam
    simultaneously. That is Q117's own fleet-killer re-entered through
    unreachability instead of tier or depletion. `try_move_to` was added to the
    start kit in the same change as "the fault-free walk" and goes unused. (Note
    the interaction with **P4**: the obvious rewrite,
    `try_move_to(closest_minable(ore))`, is itself a fault until P4 is settled.)

  - **No fallback branch.** When `exists_minable(ore)` is False, both guards
    fail closed and the program does nothing — no fault, no error template, no
    thought cloud. Start-zone nodes are finite by design, so once a colony works
    out the ore its grade-1 drill can reach, every bot walks to the depot, gets
    False from `try_deposit()`, and loops **silently, forever**: a full fleet
    pacing between depot and nothing, paying upkeep against the fleet cap, with
    zero diagnostics. Q117 removed the fault that used to announce this
    condition without specifying a replacement signal. `wander` and `explore`
    are both already in the start kit — docs/04's Feral Harvester uses the
    identical guard followed by `wander()`.

*(Resolution: both walking legs became `try_move_to` (P4-legal composition)
and the starter gained the unconditional `wander()` tail — the Feral
Harvester's idiom. Unreachable ore is a False, not a fault-loop; an
out-of-ore fleet searches visibly instead of stalling silently.)*

*(Amended: the same audit ratified the try_ pass-assignment rule this fix
created the need for — a `try_` verb resolves in its sibling's pass, spec in
[07-architecture/tick-model.md](07-architecture/tick-model.md) (`c87ee66`).
Also: this resolution note was misfiled under P27's entry by the e125abc
over-match; returned here in `c87ee66`.)*

**P8 — `investment()` still sums deleted capability tiers. FIXED (`93d6b25`).**
[07-architecture/vm.md:13](07-architecture/vm.md),
[01-language/program-colors.md:47](01-language/program-colors.md) (the ghost-exemption bullet),
[02-agents/decided.md](02-agents/decided.md), [TASKS.md](TASKS.md)

Q115 cut the Backup Core and Q111 deleted `Capability` and the tier catalog.
[01-language/program-colors.md](01-language/program-colors.md) and [02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md) were
updated to "lifetime XP plus the value of installed tools" — but the scrap
valve's spec in **07-architecture.md** (the doc an implementer builds phase 8
from), the ghost-exemption bullet in **01-language.md** *three lines below the
corrected one*, and the **Decided entry that owns the ruling** in 02-agents.md
all still read "lifetime XP plus bought capability-tier value … so a Backup-Core
reprint's tier-4 hardware is never mistaken for a rookie."

An implementer following docs/07 has no `capability_tier` field to sum, so the
hardware term evaluates to nothing and `investment()` degenerates to raw
lifetime XP. On the first sustained Steel shortfall with `rust_scraps` on, the
valve ranks a bot carrying grade-5 drill, optics and CPU **below** a rookie
hauler with slightly more Mileage — it spent the match on a pad and in transit,
so its XP is lower — recalls it, and dismantles the colony's single largest
hardware investment for a partial refund. That is exactly the failure Q105-R3
was written to close. docs/01 additionally now gives two different formulas for
the same selection twelve lines apart.

*(Amended: the TASKS.md carrier (Q105-R3 entry) was never touched by the close;
restated [~] in `c87ee66`.)*

**P9 — docs/02's *Decided* section was never swept. FIXED (`d5b561f`).**
[02-agents/decided.md:14](02-agents/decided.md) (the `100×n` curve), `:12` (Q68 upkeep),
plus the module-slot and Optics entries in the same file

The owning doc's authority under this repo's conventions still ratifies the
whole pre-sweep model: the flat **`100×n` XP curve** (`:266`), **module slots
unlocking at total-XP milestones, cap 3** (`:262`), **Optics as a slotted tool
module** (`:259`), and Q68's upkeep as "per station upgrade, module, and track
level" with a Mk2→Mk3 catalog curve (`:265`).

A tuner writing `xp.ron` from this section ships one global `100×n` curve
instead of Q123's per-track `curve_base`. Every track then climbs at one pace,
the job/ambient two-tier gap disappears, a dedicated miner takes the same ~50
minutes to L5 as the Age clock does by merely existing, and specialisation stops
beating seniority for tool licensing — the entire outcome Q123 was decided to
produce. The same section also re-introduces the unbounded `Σ levels` upkeep
term Q122 replaced, so an old fleet browns out its colony purely by being old.

*(Resolution: the XP-curve entry restated per-track (Q123) and Q68's upkeep
clause converted to Q122's bounded hyperbolic with the tool-rebased term;
the module-slot and Optics monolith entries were already gone.)*

*(Amended: the resolution's 'already gone' claim was FALSE — the Optics-module
and slot-milestone clauses survived in decided.md's line tails (truncated-grep
verification, again); actually swept in `c87ee66`.)*

**P10 — the Feral Harvester's verbatim source is still the Q117 crash-loop.
FIXED (`d5b561f`).**
[04-enemies/archetypes.md:42](04-enemies/archetypes.md)–`:48` (also [TASKS.md](TASKS.md),
[06-progression/unlock-tree.md:71](06-progression/unlock-tree.md), `:76`,
[06-progression/pacing.md:10](06-progression/pacing.md))

[04-enemies/archetypes.md:5](04-enemies/archetypes.md) states these code blocks are the archetypes'
***actual* shipped source**, and Q117's answer
([history/questions-worksheets.md:273](history/questions-worksheets.md)–`:275`) explicitly records that
`crates/sim/src/feral.rs` takes the new guarded form and that "docs/04's
verbatim sources need re-syncing." The sweep updated only docs/04's nest-claim
gate (now [04-enemies/nests-and-claims.md:9](04-enemies/nests-and-claims.md)) and left the programs untouched.

The Harvester still guards with tier-blind `exists(ore)` — which per
03-resources.md Design Rule 4 answers from **permanent map knowledge**, so it
stays True on a seam the map emptied an hour ago — binds `closest(ore).expect()`
with no minable filter, and calls the **faulting** `mine()` rather than
`try_mine()`. That is the loop Q117 measured at [history/questions-worksheets.md:168](history/questions-worksheets.md)–`:174`: closest →
`move_to` (0 ticks at chebyshev ≤ 1) → `mine` → fault → restart, ~3–4 ticks per
iteration, 2 HP per fault, a 40 HP chassis dead in about eight seconds.

So every Harvester a nest prints grinds itself into a wreck within seconds of
reaching a worked-out or over-grade vein: the PvE *economic* enemy deletes
itself, docs/04's "starve the nest (kill Harvesters) and it prints less"
counterplay becomes unreachable, and **the first Feral program a player decrypts
teaches exactly the bug** [04-enemies/archetypes.md:23](04-enemies/archetypes.md) and Q108 say a shipped source must
never teach.

*(Resolution: the Harvester carries the ratified form — minable-scoped
queries, try_ verbs, bound target, wander tail; code re-sync tracked in the
Shipped-programs task.)*

*(Amended: archetypes' verbatim-source claim now marks the code re-sync as
pending rather than asserting byte-exactness the lagging feral.rs breaks;
`c87ee66`.)*

**P11 — module slots were deleted but four places still specify them. FIXED (`d5b561f`).**
[02-agents/anatomy.md](02-agents/anatomy.md) (`| Module slots | 1 |` — the
row itself, deleted by the fix, so the line number is dropped),
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the modifier pipeline),
[02-agents/damage-faults-death.md](02-agents/damage-faults-death.md) (the salvage receipt),
[02-agents/decided.md](02-agents/decided.md);
[03-resources/the-tree.md:94](03-resources/the-tree.md) (Lens); [07-architecture/world-state.md:6](07-architecture/world-state.md)

docs/06 deleted the entire slotted-module catalog (Optics and Backup Core
entries plus the swap-economics paragraph) and 02-agents/decided.md's entry
dropped "slots 1" from the print floor. Left behind: the universal base statline
still prints `| Module slots | 1 |` (`:24`); the modifier pipeline still runs
through "Upgrade Station purchases **+ slotted modules**" (`:32`); the salvage
receipt still derives from "slotted modules … swapped-out modules drop off — Q72
swap rules" (`:62`), citing a rules paragraph this sweep deleted; and `:262`
still rules slots unlock at total-XP milestones, cap 3.

Worst of the set is `:259` — "On a one-slot rookie, Optics is the whole build —
a dedicated prospector that gave up its ability to work" — which flatly
contradicts the sensor-range row the same sweep wrote at `:51`: "optics is one
of the ten tools since Q111 … so no rookie ever trades its working ability for
eyes." A reader cannot tell whether a bot has a slot, whether Optics consumes
it, or how salvage values a part that no longer exists.

Separately, [03-resources/the-tree.md:94](03-resources/the-tree.md) still routes the whole Lens
supply chain into "The **Optics module** (2 Lens + 1 Bronze)" — a deleted
catalog entry — leaving **Lens with no priced consumer anywhere in the design**.

*(Resolution: statline row, pipeline clause, and the salvage receipt's
slot/swap clause deleted (the receipt carrier was in stat-sheet.md, not
damage-faults-death.md as cited); Lens retargeted to the optics tool's upper
grades — a priced consumer via the ratified sensing chain, no ruling needed.)*

*(Amended: four more carriers survived the close — the pipeline's slot-order
tie-break, anatomy's identity and floor-statline clauses, the 02 doorway row,
and the Q72 receipt clause in 03-resources/decided and reprinting; swept in
`c87ee66`.)*

**P12 — two identical "Cycles per tick" rows with contradictory growth sources.
FIXED (`93d6b25`).**
[02-agents/stat-sheet.md:15](02-agents/stat-sheet.md) vs `:20` (also
`:66`; [03-resources/the-tree.md](03-resources/the-tree.md);
[06-progression/scopes.md:20](06-progression/scopes.md))

Line 40 says cycles per tick is grown by "**Upgrade Station** (walk there, pay
Chips)" — a flat buy — while line 45 says it is grown by the "**CPU tool**
(Upgrade Station), licensed by the **Processing track**." Q111 moved cycles off
flat buys onto the tool/licence model ([02-agents/anatomy.md](02-agents/anatomy.md): "Cycles
per tick is the CPU tool"), so line 40 states the superseded model.

Before this sweep the second row carried the suffix "— see the Processor
capability" in its Stat column, which marked it as the cross-reference rather
than a second canonical row; the edit deleted the marker, leaving **two
indistinguishable canonical rows for the single most contested stat in the
game**. An implementer building `stats.ron` from the sheet gets two conflicting
growth sources for one stat, and line 45 still closes in the deleted model's
language ("joins Q105's capability model — buy the tier, then sharpen it by
working").

**P13 — `repair()` gates the rescue verb on both the new grade and the deleted
Building tier. FIXED (`93d6b25`).**
[01-language/builtins.md:26](01-language/builtins.md) (the `repair()` row)

The builtin row was edited in place without deleting the old clause, so one cell
now reads: "field repair of a wreck needs **a build tool of grade ≥ 2** (Q105-R2,
restated for Q111); on a wreck = field repair (the rescue verb), which needs
**Building tier ≥ 2** (Q105-R2 — the replacement for the deleted build-tool
gate)." Q111 deleted capability tiers entirely
([history/questions-worksheets.md:22](history/questions-worksheets.md): "TIERS
ARE REMOVED"), so the trailing clause gates the rescue verb on a stat no bot has,
and its parenthetical asserts the opposite of the sentence in front of it.

This is the **sole surviving "Building tier" reference in docs/01–09** — the one
cell the mechanical propagation missed.

**P14 — the `XP gain` stat row was deleted, but two quirks still modify it.
FIXED (`d5b561f`).**
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the deleted row and the canonicity rule),
[02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md);
[09-quirks/catalog.md:19](09-quirks/catalog.md), `:39`;
[07-architecture/tick-model.md:29](07-architecture/tick-model.md);
[04-enemies/capturing-wrecks.md:5](04-enemies/capturing-wrecks.md); [TASKS.md](TASKS.md)

[02-agents/stat-sheet.md](02-agents/stat-sheet.md) declares the sheet canonical: "Anything anywhere
in the design that makes one bot better or worse than another — hardware, XP
perks, quirks … modifies a row on this sheet; **if an effect can't name its row,
it isn't a stat effect.**" The sweep deleted `| Survival | XP gain | 100% |
Learning track |` along with the Learning track — but 10x Developer (+15% XP
earned, all tracks), Tech Debt (−15% XP earned), [history/questions-worksheets.md:468](history/questions-worksheets.md) ("quirk
`XpPct` effects … stay"), docs/07's "any per-bot XP-gain multiplier (quirks
only) applies at its start-of-tick value" and 02-agents/xp-and-specialization.md itself all still
specify it.

An implementer building `stats.ron` from the canonical sheet ships no XP-gain
stat and the two quirks have nothing to apply to; the modifier-pipeline position
and the pessimistic-rounding rule for that multiplier are gone with the row.

*(Resolution: the row is restored as quirks-only — start-of-tick value,
pessimistic rounding — per the recorded 'XpPct effects stay' intent.)*

**P15 — the disconnect ruling's footnote points PvP disconnects at "open
questions", but they are decided two bullets down. FIXED (`93d6b25`).**
[08-multiplayer/decided.md:11](08-multiplayer/decided.md)

The colony-keeps-running ruling closes with "(Decided for co-op / non-harm
play; PvP disconnects need more thought — see open questions.)" — stale since
"PvP disconnects: free farm until reconnect" was ratified in the same Decided
section; there is no open question to see. The fix is a one-clause pointer
("see below"). Registered rather than silently reworded so the 04–09 doc
split stays a byte-exact move of decided text.

**P16 — the Drone's and Stinger's verbatim sources still check-then-act
across a blocking `move_to`, the pattern Q110 ruled out. FIXED (`93d6b25`).**
[04-enemies/archetypes.md:18](04-enemies/archetypes.md)–`:19` (Drone),
`:31`–`:32` (Stinger); the ruling inside Q117's answer
([history/questions-answered.md](history/questions-answered.md))

Q110's ruling — "bind once, never check-then-act", recorded inside Q117's
answer and cited by [01-language/syntax-tiers.md:42](01-language/syntax-tiers.md)
("the bug Q110 ruled against") — rules out re-querying a target around a
blocking verb, whose tens-of-ticks window "is what made Q110's Feral race a
systematic bug"
([history/questions-worksheets.md:247](history/questions-worksheets.md)–`:248`).
The ratified Drone and the ratified Stinger both do exactly that — the
byte-identical pair `move_to(closest(enemy).expect())` then
`attack(closest(enemy).expect())`. Same left-behind class as P10's Harvester.
Until the sources are re-synced, the first combat programs a player decrypts
teach the racing form Q108 says shipped source must never teach. (The doorway's Q110
open-question entry was retired with the split — the question is answered;
this register entry replaces it.)

**P17 — the "hardware is Chips-priced" shorthand survives in four places,
contradicting the ratified tool pricing it summarizes. FIXED (`93d6b25`).**
[06-progression/scopes.md:10](06-progression/scopes.md) (the per-match row)
and `:20` (the three-scopes list),
[06-progression/unlock-tree.md:67](06-progression/unlock-tree.md) (the axis
sentence), [02-agents/decided.md:11](02-agents/decided.md) (the compute-stats
ruling's "(Chips — …)" gloss); the pricing in
[06-progression/upgrade-station.md:30](06-progression/upgrade-station.md)–`:59`

All four lines gloss hardware buys as Chips-priced, but the owning part
prices tool grades by resource role — *Bronze arms, Chips think* — across
Steel, Bronze, Sand/Glass/Lens, Wire and Silver, with Chips entering only at
CPU grade 4, and deliberately starts every flat capacity buy on **Wire**
rather than Chips (upgrade-station.md: "These start on **Wire** rather than
Chips deliberately"). A reader taking the shorthand at face value concludes
Chips are the hardware currency and mis-plans the material gating of nine of
ten tools; the same shorthand in the 06 doorway intro was corrected in the
2026-08-01 sweep. The fix is a wording pass on the four lines (e.g.
"hardware (Upgrade Station)" or "hardware (materials by role)"), not a
pricing change — closing this entry requires re-grepping for the shorthand,
not just fixing the lines listed here.

**P18 — the hijack ruling still credits the deleted Boot XP track. FIXED (`d5b561f`).**
[04-enemies/capturing-wrecks.md:5](04-enemies/capturing-wrecks.md) ("counts
as a rescue boot for its Boot track");
[02-agents/decided.md:11](02-agents/decided.md) ("Boot and Learning were
later cut"),
[02-agents/xp-and-specialization.md:68](02-agents/xp-and-specialization.md)
(Boot "never once awarded"),
[07-architecture/tick-model.md:29](07-architecture/tick-model.md) (the
ten-track settle order)

Q111's sweep cut the Boot track from the XP model — the tick's XP settlement
runs exactly ten tracks and 02-agents records the cut — but the hijack
ruling moved into capturing-wrecks.md still awards the stolen bot's hijack
boot as "a rescue boot for its Boot track." An implementer building the
hijack path from docs/04 credits XP to a track that does not exist: either
the code grows an eleventh track (a hash-affecting divergence between
implementations — the desync class) or the clause is silently dropped with
no record. Same left-behind class as P14's Learning-track modifiers; the
clause needs a ruling-side sweep (drop the award, or re-home it on a
surviving track), not a silent reword.

*(Resolution: the rescue-boot award clause is dropped with its track.)*

**P19 — the Q77 Command inventory omits `ClaimNest` and `RazeNest`. FIXED (`93d6b25`).**
[07-architecture/world-state.md:32](07-architecture/world-state.md) ("the
ONLY external inputs to sim (Q77: list completed"),
[08-multiplayer/decided.md:17](08-multiplayer/decided.md) (Q86 names both);
[TASKS.md](TASKS.md)

The inventory declares itself complete, but Q86's authorization ruling
explicitly lists `ClaimNest` and `RazeNest` among the cross-faction commands
the relay binds to the sender's faction, and TASKS.md specifies their
effects ("RazeNest banks the Data bounty, ClaimNest converts it").
`ClaimNest` appears nowhere in docs/07. An implementer building the command
layer from the canonical inventory ships a sim in which nest conversion —
the gate on every printer/color past the second — has no input path; and
because Commands are the lockstep input stream, implementations that
disagree here also disagree on Q86's forgery-protection set.

**P20 — the Hiding perk is a second linear-uncapped perk, contradicting
Q121's own rule. FIXED (`3e21e89`).**
[02-agents/xp-and-specialization.md:65](02-agents/xp-and-specialization.md) and
[02-agents/stat-sheet.md:26](02-agents/stat-sheet.md) ("−1 signature/level,
tuning") vs
[02-agents/xp-and-specialization.md:15](02-agents/xp-and-specialization.md)
("none of them is linear-per-level")

Q121 converted perks to bounded shapes because the ladder is uncapped; P6
records Flinch as "the one surviving linear perk." Hiding is a second
survivor, registered nowhere: signature falls 1 per level, and heard-at
distance (their hearing radius + this signature) floors at 1 — so a Hiding
bot around level 6–7 against base hearing 7 is heard only at adjacency,
everywhere, permanently. That deletes the movement-noise detection layer
(Sentry early warning, creeping's trade, signature quirks) for veteran
infiltrators — the "switch fog of war off at a reachable level" failure Q121
names as the reason the rule exists. Same two fixes as P6: convert to the
bounded hyperbolic, or floor it at a nonzero signature.

*(Resolution: converted to Q121's bounded hyperbolic with `max_quiet` tuned
below base hearing — hearing detection never switches off. Ruled together
with P6, leaving zero linear perks.)*

**P21 — Q117's branching-at-start never propagated to three "`if` is an
unlock" passages. FIXED (`93d6b25`).**
[06-progression/unlock-tree.md:76](06-progression/unlock-tree.md) (Design
Rule 2: "The player wants `if` because they *felt* its absence") vs the same
file's START node (`:7` grants **if / elif / else** at game start);
[01-language.md:6](01-language.md) ("Construct gating — `if`, loops,
variables, `def` are *unlockable features*");
[00-overview.md:66](00-overview.md) (glossary Construct entry)

Q117 granted branching at game start (the guarded starter needs it). The
tree's START node was updated; the prose was not: the 01 doorway invariant
and the overview glossary still name `if` as the flagship unlockable, and
Design Rule 2 still sells the tree with the example the ruling deleted. A
data author pricing constructs from the doorway adds a research cost to
branching — no tree node exists for it — and a fresh account then cannot
load the shipped Tier-0 starter, which opens with `if exists_minable(ore):`.

*(Amended: three more carriers survived — scopes' construct row, unlock-tree's
reading note, archetypes' Stinger header; swept in `c87ee66`.)*

**P22 — the canonical hurt window faults whenever no Repair Bay is in range;
whether a faction's own structures are map knowledge is undecided. FIXED (`09c3e62`).**
[01-language/signals-and-logging.md:18](01-language/signals-and-logging.md)
(`move_to(closest(repair_bay).expect())`)

Resource nodes have a decided knowledge model (a seen tile is fully known;
queries answer from `known_nodes`); structures have none. If
`closest(repair_bay)` answers from perception, the canonical hurt handler
faults the moment a bot is hurt beyond sensor range of a bay — `.expect()`
on Err inside a running handler is the double-handle wreck path (P4's
class), shipped as the recommended idiom. If it answers from permanent
knowledge, no doc says so, and the two readings diverge — hash-affecting.
Needs one ruling: do a faction's own structures (or all discovered
structures) count as map knowledge for query builtins?

*(Resolution: queries answer from faction knowledge — own structures always,
foreign as last observed via a phase-5 known-structures memory. The canonical
hurt window gained its `exists` guard; ruling in 05-terrain/decided.md.)*

*(Amended thrice — final form: the third audit showed the `faction=` selector
design generating contradictions faster than patches closed them; the ruling
simplified to the **knowledge pool** (own colony state + granted allies',
current by construction, foreign structures not query-reachable — Q126 opened
for a future surface; no per-faction memory, no new hashed state) in
`95c73c8`. Earlier: the second audit scoped `faction=` to structure/designation
kinds only, bound the selector constants, brought blueprints into the ruled
class, and pooled the memory under the ally vision grant (`6686866`).
First: the ruling lacked an ownership filter (queries default `faction=own`,
foreign memory is opt-in — `c87ee66`) and had not propagated to fog-of-war.md,
the stat-sheet sensor row, or 01-language/decided.md; both fixed in `c87ee66`.)*

*(Third-audit addendum, 2026-08-02: two stale carriers of the unguarded
retreat idiom survived every sweep — `05-terrain/tiles.md`'s Crystal Field
cell and `05-terrain/map-generation.md`'s chokepoint idiom; both now carry
the `exists` guard.)*

**P23 — the execution model still grows compute through the deleted
"Processor capability (tier × level)". FIXED (`93d6b25`).**
[01-language/execution-model.md:29](01-language/execution-model.md)
("Compute grows instead through the **Processor capability** (tier ×
level — [02-agents.md](01-language/../02-agents.md))")

Q111 removed tiers and the capability model; cycles per tick is the CPU tool
(grades 1–5, licensed by the Processing track —
[02-agents/anatomy.md](02-agents/anatomy.md),
[06-progression/upgrade-station.md](06-progression/upgrade-station.md)). The
Q100 ruling's closing sentence — in the execution-model part an implementer
of the cycle economy reads first — still cites the deleted formula. Not
covered by P8 (the investment formula) or P12 (the stat-sheet rows).

**P24 — the 01-language doorway's parts table says "Tiers 0–6"; the part
defines Tiers 0–7. FIXED (`93d6b25`).**
[01-language.md:17](01-language.md) vs
[01-language/syntax-tiers.md:144](01-language/syntax-tiers.md) ("## Tier 7 —
Channels") and [01-language/builtins.md:41](01-language/builtins.md)
(`send` "Requires Tier 7")

The ownership table's tier count predates the channels tier. A gating or
renumbering change made against the doorway's 0–6 ladder drops or misplaces
the parse-time gate on `send`/`receive` — a deploy-validation divergence
between peers, and the doorway-drift failure the split convention exists to
catch.

**P25 — two quirks modify a "boot ritual" duration that names no stat-sheet
row. FIXED (`d5b561f`).**
[09-quirks/catalog.md:22](09-quirks/catalog.md) (**Hot Reload**: "boot
ritual half as long — [02-agents.md] stat sheet") and `:52` (**Windows
Update**: "boot ritual twice as long");
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the canonicity rule)

The sheet's own rule is "if an effect can't name its row, it isn't a stat
effect." No boot-duration row exists (Print time and the hurt/Damaged lines
are different rows), and Hot Reload even cites the stat sheet as its home.
Same left-behind class as P14's XP-gain quirks, different stat: either the
sheet gains a boot-ritual-duration row (with modifier-pipeline position and
rounding rule) or the two quirks need re-speccing.

*(Resolution: a Boot-ritual row joins the sheet, quirks-only, so both
quirks name a row per the canonicity rule.)*

**P26 — the Scouting income row still asserts "no seen-tile set", which Q94
overturned. FIXED (`93d6b25`).**
[02-agents/xp-and-specialization.md:13](02-agents/xp-and-specialization.md)
("Q83 — sim events; no seen-tile set, so eyes-only fog stays stateless") vs
[05-terrain/decided.md:12](05-terrain/decided.md) ("Seen tiles are sim
state", answers Q94) and
[07-architecture/tick-model.md:28](07-architecture/tick-model.md) (the
phase-5 per-faction map writes)

Q94 made the per-faction known-tiles set hashed sim state; the Scouting
row's parenthetical still asserts the pre-Q94 stateless model. An
implementer deriving discovery events from an ad-hoc structure instead of
the phase-5 writes diverges on when "node discovered" fires — divergent
Scouting XP and Data awards are a replay-hash desync.

*(Amended: a second carrier — the Data-income clause "seen-set-free, like
Scouting" in [03-resources/decided.md:18](03-resources/decided.md) — was
missed by the first close and shut in `0060a47`.)*

**P27 — solid structures have no slot in the ratified tile-composition
model. FIXED (`a2a81ff`).**
[05-terrain/tile-composition.md:9](05-terrain/tile-composition.md) ("An
unwalkable building (exclusive)... the Barricade today — owns its tile
outright: it shares with *nothing*"); Q98's Pump in [TASKS.md](TASKS.md)
(both tiles solid, the intake *in* a Water tile)

The physical model is a strict either/or: exclusive unwalkable building, or
walkable ground stack. The Pump intake is a solid structure standing in
Water it must keep (it pumps it) — a share the shares-with-nothing class
forbids — and solid structures generally (Depot, printers, nests: the tiles
Q120's displacement BFS excludes) are assigned to neither class. Needs one
ruling on where structure solidity lives (tile-kind replacement like the
Barricade, or a contents slot the model currently omits); the answer decides
whether paint and overlays survive under a structure and what demolition
leaves behind.

*(Resolution: occupancy layer — solid structures are entities standing on
the ground stack, solidity from the structure registry, stack inert not
erased beneath; Barricade keeps its Q99 tile-kind exclusivity. Ratifies the
code's existing structure_at shape.)*

**P28 — the function-block scope row still gates some functions on a "tool
module". FIXED (`93d6b25`).**
[06-progression/scopes.md:19](06-progression/scopes.md) ("some also need a
tool module on the bot")

Q111 deleted the slotted-module catalog (P11 records the other survivors);
the per-bot gate on function blocks is tool *grade* (e.g. `hijack()` needs a
build tool of grade ≥ 2). The row sends readers hunting a module catalog
that no longer exists anywhere in the design. A one-clause fix, registered
rather than silently reworded because the text is ratified.

**P34 — the Q56 granularity ruling still stores XP in deci-units, which Q111
superseded and which cannot express the shipped Age income. FIXED
(`2e768b6`). ⚠HASH**
[02-agents/decided.md:8](02-agents/decided.md) (the Q56 entry: "cargo/progress/move
— and, since round 4, XP — in deci-units") vs. **the same file's `:14`**
("level *n* costs `curve_base × n` additional **centi-XP**"),
[02-agents/xp-and-specialization.md:70](02-agents/xp-and-specialization.md)
("**XP stores CENTI-points** in an `i64`"),
[02-agents/stat-sheet.md:58](02-agents/stat-sheet.md) ("**XP stores centi-points**
— one place finer than the rest"), [02-agents.md:32](02-agents.md) (the doorway),
[07-architecture/tick-model.md:29](07-architecture/tick-model.md) ("Awards are in
**centi-points**"), [TASKS.md](TASKS.md) (*XP core*: "`i64` **centi-points**")

Q56's round 4 filed XP into the deci group on 2026-07-14. **Q111 moved it out**
— "Centi-points (`i64`), **replacing deci**" — because the `gain_carry` /
`learning_carry` fields existed precisely to compensate for deci being too coarse
for a sub-100% cut of a small drip, and Q111 deleted them. The sweep updated five
carriers and missed the entry that *owns* the ruling, so under this repo's
convention the stalest text sits in the most authoritative place — the P7/P9
failure mode exactly.

The consequence is not cosmetic. `curve_base` values are published in the storage
unit: Mining's 4,000 (P2's corrected figure) against 10 centi/tick gives L5 =
15 × 4,000 = 60,000 centi = **6,000 ticks ≈ 10 minutes**, the job-track target
Q123 exists to produce. A tuner building `xp.ron` from the Decided section stores
deci with the same published numbers and gets **60,000 ticks ≈ 100 minutes** —
10× the target, and *behind* the 50-minute ambient Age clock, which re-enters the
seniority-beats-specialisation failure P2 was opened to close. Worse, Q123's Age
income (1 XP per 50 ticks = **2 centi/tick = 0.2 deci/tick**) is not a deci
integer at all: in deci storage it truncates to zero and Age never levels, or it
needs the carry field Q111 deleted. `unit_scale` is also a rounding input to the
modifier pipeline, so the two XP-gain quirks round differently on two peers.

*Why three audits walked past it.* (1) The stale text matches the **shipped
code** — `crates/sim/src/xp.rs` still stores deci (`age_deci_per_tick`,
`mileage_deci_per_tile`, `processing_per_op_deci`, one global `curve_base`, a
`level_cap`), because the centi migration is real but unbuilt; anyone checking
the entry against the code called it correct. (2) The *Checked and cleared* entry
above ("The XP-core task mixes centi and deci units") verified the adjacent
question in TASKS.md and created an exemption that read as covering the whole
topic. (3) The contradiction is **intra-file, six lines apart**, which no
single-term grep surfaces.

*(Resolution: the Q56 entry restated — XP in centi-points, with Q111 recorded as
superseding round 4 and the reason kept (deci cannot express the Age income), and
`unit_scale` ownership pointed at the stat sheet. The last two deci figures in
the XP path — TASKS.md's "Age income → 0.2 deci/tick" and
xp-and-specialization.md's "0.2 deci = 2 centi per tick" — restated in centi, so
the *Checked and cleared* exemption is retired with them. A doorway invariant now
names the stat sheet as the sole authority on any row's unit. **No replay hash
moves**: XP core is unbuilt and already carries ⚠HASH + units migration, so the
fix was free to land now and would not have been after that task ships.)*

**P35 — "blocking burns the budget" is specified two incompatible ways, and the
docs never state which. FIXED (`33b1de8`).**
[01-language/execution-model.md:33](01-language/execution-model.md) (the owning
rule: "**burns** its per-tick budget"),
[01-language/syntax-tiers.md:170](01-language/syntax-tiers.md) ("its per-tick
cycle budget burns while it waits"),
[01-language/decided.md:31](01-language/decided.md) ("**Blocking burns
cycles**"), [01-language.md](01-language.md) (the doorway invariant),
[TASKS.md](TASKS.md) M11 ("Blocking burns the budget (M5's rule)")

Every carrier says the budget "burns" while blocked and none says what happens to
cycles **banked before** the block. Two readings, both defensible from the prose:
*forfeit the grant* (stop earning, keep the bank) or *zero the budget* (blocking
empties it). They diverge by up to a full `bank_cap` — 25 cycles today, ~100
under Q101's queued flat ceiling — **on every wake from every action**, and since
actions block permanently (Q100) that is most of every bot's life. Two
implementations of the same spec desync in lockstep, the P3 class exactly.

The shipped VM already picks a side: `Vm::grant_centi` returns early when
`State::Blocked` and never touches `self.budget`, and the `stall_cost()` comment
says so outright — "a bot waiting on an action **burns its grant rather than
banking it**". So the code was unambiguous and the design docs were not, which is
the reverse of the usual direction and the reason no audit caught it: every
carrier agreed with every other carrier, and all of them were incomplete in the
same place.

*(Resolution: ratified as the code already behaves — **what burns is the grant,
never the bank**. A bot receives no grant at all while blocked; cycles banked
before it are retained, frozen, and still there on wake. Stated in
execution-model.md with a worked example, and the three restatements plus the
doorway invariant now carry the clause pointing at the owner. **No code change
and no replay hash moves** — the fix is the spec catching up to the
implementation. Found by a comprehension question about the execution model, not
by a sweep: the ambiguity is invisible to grep because no two carriers
contradict each other.)*

**P36 — the canonical kind-constant inventory diverges from the shipped registry
in three ways while asserting it is complete. FIXED (`a41ec41`). ⚠HASH**
[01-language/types-and-env.md:13](01-language/types-and-env.md) ("**Every
resource and every registry kind gets one** (Q79, completed round 4)") vs.
`crates/sim/src/host.rs` (`KINDS`), [05-terrain/decided.md:17](05-terrain/decided.md)
(Q99), [TASKS.md](TASKS.md) (`closest(blight)`; *Barricade HP*)

Kind constants are **pre-bound globals**, so an unknown name fails at
parse/deploy rather than at runtime: two peers built from different lists
disagree about whether a program *loads at all*, which is divergence before the
first tick rather than drift during play. Three defects in one list:

  - **`blight` was missing.** It ships today — `KINDS` carries it with a comment
    citing this doc's own rule ("the creep's heart (M8-C) — attackable, so it
    must be findable") and TASKS.md documents `closest(blight)` as live. The
    omission also dropped the rule that goes with it: `closest(blight)` is
    **perception-ungated**, and Q99 leans on exactly that contrast when it
    specifies barricades as "perception-gated like structures, *unlike the creep
    front*". So the owning doc was missing both an entry and the distinction it
    anchors.
  - **`barricade` was missing.** Q99 ruled it on this same rule, verbatim. Listed
    now with its domain flagged open: Q127 rules what it may *find* (P29), never
    whether it exists, and the `KINDS` entry stays build-blocked meanwhile.
  - **`chip` should be `chips`.** Every other carrier — the material in
    03-resources, `Resource::Chips`, the string `"chips"`, the Foundry recipe,
    and `KINDS` — says plural; only the canonical inventory said singular, while
    `gold_chip` is singular in both. A one-character slip that makes
    `cargo_count(chip)` name nothing.

*(Resolution: all three swept into
[types-and-env.md](01-language/types-and-env.md), with `blight`'s ungated
perception and `barricade`'s gated-but-unbuilt status written as sub-bullets so
the two rules travel with their entries. **No code change and no hash movement**:
`blight` and `chips` were already right in `KINDS`, and `barricade` stays
unbuilt. The reverse gap found in the same pass — shipped structures with no
constant — is **P37**, open above.)*

*(Amended 2026-08-14 by **P40**: the third evidence bullet overstated its sweep —
the Foundry recipe spells the material "Chip", singular, in every one of its
live-spec positions, so "every other carrier … says plural" was false. The ruling
stands — `chips` is correct in the inventory — but it rests on the material name
alone, not on agreement with the recipe.)*

**P39 — the kind-constant registry claims every placement but `blight` is
perception-gated, against a ruling carried in ten passages across seven files.
FIXED (`82d30f4`).**
[01-language/types-and-env.md:17](01-language/types-and-env.md) (the `blight`
sub-bullet as P36's fix wrote it) vs. the P22 ruling at
[05-terrain/decided.md:22](05-terrain/decided.md), the doorway invariant at
[05-terrain.md:43](05-terrain.md),
[01-language/decided.md:29](01-language/decided.md) and
[01-language/builtins.md:34](01-language/builtins.md).

P36 gave `blight` its missing entry and stated its ungated perception correctly,
then closed the sub-bullet with a universal: "Every other findable placement is
gated." P22 rules the opposite for the largest class in the list — structure and
designation kinds resolve from the faction's knowledge pool, "your own — colony
state, always current, **no perception needed**" — and Q74 puts discovered nodes
outside perception too ("a known vein is a fact, not a perception"). Of the nine
constants the bullet introduces, `blueprint` is pool-resolved, `enemy` carries
the heard-only position-only rule, `wreck` and `nest` are own-or-seen, and
`black_box` and `cache` are seen-only: six of nine carry a rule, against the
lead-in's "Two of these carry rules the others don't". The sentence also
contradicted the **Decided** section of its own doc, one file over.

Two aggravators. The claim sat in the **registry**, which owns *which constants
exist*, not *what they reach*; restating a domain model it does not own is what
let a single edit drift from ten agreeing carriers. And the barricade sub-bullet
beside it carried "**perception-gated like a structure**", copied from the Q99
ruling at [05-terrain/decided.md:17](05-terrain/decided.md) — the losing half of
**P29**, whose whole substance is that Q99's phrasing and the P22 bullet five
lines below it cannot both be true. P36 spread that contradiction from one file
to two.

*(Resolution: the registry stops restating the model and cites it — the domains
are owned by [decided.md](01-language/decided.md) and
[builtins.md](01-language/builtins.md), and differ by kind. `blight`'s note
survives as the one perception-ungated placement; `barricade` keeps its Q79/Q99
grounds and its Q127 block but drops the gating claim, so **P29 is back to one
carrier** and nothing here pre-empts Q127. The miscount retires by not making a
count. **No code change and no hash movement** — doc text only. The code-side
divergence the audit turned up alongside it is the unbuilt P22 task, whose
implementation notes were amended the same day.)*

**P40 — P36's evidence claims the Foundry recipe spells the material plural; it
spells it "Chip" in every position. FIXED (`6ded0fe`).**
This file (P36's third evidence bullet, in the Fixed log) and
[01-language/types-and-env.md:14](01-language/types-and-env.md) (the same
justification) vs.
[03-resources/structures-and-start.md:23](03-resources/structures-and-start.md),
[03-resources/the-tree.md:30,91,92,95](03-resources/the-tree.md) and
[03-resources/decided.md:12,16](03-resources/decided.md).

P36 corrected the kind-constant inventory from `chip` to `chips` and justified it
with "Every other carrier — the material in 03-resources, `Resource::Chips`, the
string `"chips"`, the Foundry recipe, and `KINDS` — says plural." The Foundry
recipe says **Chip**: "1 Silver + 2 Crystal + 1 Wire → 1 Chip" and "1 Chip + 1
Gold → 1 Gold Chip". `types-and-env.md:14` carried the same claim.

A sweep of docs/03 shows this is not drift. The corpus splits cleanly by
grammatical position — **Chips** wherever the material is named (the-tree row
header, the refined-goods list, "Bronze arms, Chips think", the harvest tiers)
and **Chip** in all seven recipe and count positions. The code makes no such
distinction: `crates/sim/src/resources.rs:78` names the material `"chips"`, and
`resources.rs:165` is `Recipe { name: "chips", …, output: (Resource::Chips, 1) }`
— one unit, called "chips". So P36 did not find a typo. It found the seam between
a documentation convention and a code convention, and reported them as agreeing.

The consequence outlived the fix. P36's stated reason was that `chip` "makes
`cargo_count(chip)` name nothing" — but seven live-spec passages still teach the
reader the noun *Chip*, so the trap was closed in the registry and left open in
the docs a player actually reads.

*(Resolution: the split is **ratified, not swept** — "Chips" is the material, "a
Chip" is one unit, written where the material is owned
([03-resources/decided.md](03-resources/decided.md)), with the consequence that
matters — **the constant never inflects** — stated at the constant itself
([01-language/types-and-env.md](01-language/types-and-env.md)). The false
justification is struck from both carriers, and P36 carries an amendment note.
Considered and rejected: converging the docs on plural ("→ 1 Chips") reads badly;
renaming the constant to `chip` would overturn a twelve-day-old ruling for a
cosmetic gain — verified cheap, since no golden fixture references `chips` and
only `crates/sim/tests/economy.rs:507` would change, but not worth it. **No code
change and no hash movement.**)*

**P41 — the glossary enshrines two names for the Printer, and the corpus uses
both. FIXED (`<hash14>`).**
[00-overview.md:67](00-overview.md) (the glossary row, formerly
"**Fabricator / Printer**") and twenty further mentions across thirteen live
docs, plus eleven in `crates/`, vs. `printer` as the sole identifier in code
(`World.printers`, `PrinterState`, `Command::PlacePrinter`, `KINDS`).

One structure, two names, with the glossary *ratifying* the split rather than
resolving it — so neither name was wrong and both kept spreading. The two
appeared in the same sentence more than once
([01-language/program-colors.md](01-language/program-colors.md): "every slot is
embodied in a physical **Printer** (Fabricator)";
[03-resources/structures-and-start.md](03-resources/structures-and-start.md):
a catalog row headed "**Fabricator** (printer)", and a starting state listing "1
working Fabricator (the **Green** printer)"). Pyrite only ever knew `printer`, so
every "Fabricator" in the docs named a thing no program can say.

*(Resolution: **Printer** everywhere, on the owner's ruling. Swept across
thirteen live docs and five code files — comments and test messages only, no
identifier touched, so no behaviour change and no hash movement. The glossary row
is **Printer** alone. Terms of art went with it: the *Red-Fabricator pattern* is
now the **Red-Printer pattern**, and Q84's *Fabricator trickle* the **Printer
trickle**. Closed records keep their wording — `docs/history/` and this
register's own Fixed entries still read "Fabricator", the standing exception that
lets history contradict current design. Raised in `docs/personal_problems.md`,
item 1.)*

**P42 — "allegiance" names two unrelated things: a nest's tarot rank and a
building's owner. FIXED (`<hash14>`).**
[00-overview.md:75](00-overview.md) and
[04-enemies/allegiance.md](04-enemies/allegiance.md) (the Major Arcana rank) vs.
[QUESTIONS.md](QUESTIONS.md) (Q127's title and worksheet),
[TASKS.md](TASKS.md) (Barricade HP — "the registry's allegiance field") and this
file (P29's Q127 note).

**Allegiance** was glossary-defined and given a file of its own as *a Nest's rank
0–21 on the tarot Major Arcana* — a difficulty-and-personality axis with a 22-row
table, a doorway invariant ("Allegiance is who a nest is; escalation is how awake
it is"), and `crates/sim/src/world.rs:342` carrying the same sense in code. Q127,
opened 2026-08-02, then took the same word for **which faction owns a building**,
a meaning with nothing in common with the first. Eleven uses carried the
established sense, seven the new one — all seven downstream of Q127. A reader
meeting "does every building carry an allegiance" beside a file that ranks nests
by tarot card cannot tell the two apart, and Q127 is what the whole barricade/P29
thread waits on.

*(Resolution: the **Arcana meaning wins** — it is glossary-defined, older, owns a
file name and a doorway invariant, and matches the code. Q127's usage becomes
**faction** / **owning faction**, which costs nothing because Q127 is open and
unratified, and which is what the sim has called it all along
(`Structure.faction`, `Blueprint.faction`, `Depot.faction`). Q127's *substance* is
untouched — only its vocabulary. The glossary gains a **Faction** row and a note
that Allegiance has the one meaning only; that row also separates the three terms
orbiting it — a **colony** is a faction as an organisation, a **team** is a set of
allied factions, and a **color** is a program slot, not a side. Raised in
`docs/personal_problems.md`, item 2 — "Alligence is really what team the structure
or bot is on". The diagnosis was right; *team* was not available as the
replacement, being already taken for alliance groups in
[08-multiplayer/code-visibility.md](08-multiplayer/code-visibility.md).)*
