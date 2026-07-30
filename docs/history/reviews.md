# Review rounds (archive)

The six closed review rounds, moved out of [TASKS.md](../TASKS.md) on
2026-07-29. **Oldest first.** Every finding in every round below was fixed at
the time; nothing here is outstanding.

Kept for provenance — these rounds record which defect classes this codebase
actually produces, which is worth knowing before the next review. Questions
opened by a round (e.g. Q86–Q91 from the 2026-07-17 spec-conformance pass) were
answered later; see [questions-answered.md](questions-answered.md).

---

## Review round 2026-07-16 (xhigh, M10–M13 working tree) — all 15 findings fixed

Crashes/losses: `PostRequest` clamps on a char boundary (a mid-codepoint `String::truncate`
was a remote-triggerable lockstep panic); `LockstepPeer::submit` claims a fresh tick per
frame (`next_submit`), so a stalled barrier no longer overwrites queued commands. Harm &
perception gates: guard/escort swings now pass `harm_allowed` + never hit declared allies +
require the victim in the perception cloud; `attack()`'s victim lookup covers nests (a
CLAIMED nest is the claimant's property on Non-PvP); `move_to`'s stale-handle exemption is
owner-scoped (foreign nests need eyes, killing the entity-id fog sweep); channel verbs
accept faction 0 (per-site range checks replaced the shared `> 0` guard; out-of-u8 factions
fault instead of truncating). Hash coverage (⚠HASH, golden regenerated — every replay hash
moves; fixture behavior unchanged): `hash_bot_data` now hashes EVERY BotData field and is
shared by live bots and wrecks (upgrade/module identity, per-track XP, quirks, carries,
rng_program, crash_seen — plus in-flight `Action`/`ActionRequest` state incl. channel
`waited`/parked payloads via exhaustive `hash_action`/`hash_request`); `harm_enabled` +
vote plumbing and `BlackBox.pos` joined phase 9. Game rules: hijack AND rescue hold at the
fleet cap (ghosts exempt — the countdown keeps burning, so a stuffed roster can lose the
race); `try_send`/`try_receive` are jammed from the CALLER's tile too (Corruption blocks
both ways, matching the blocking verbs); vision grants copy from a pre-grant snapshot
(never transitive, faction-number independent); nests are solid (structure_at,
A* blocked set, spawn tiles, PlacePrinter's free check); repair pays Building XP only for
work actually done. Regression tests in multiplayer/lockstep/channels/ferals/wreckrace/
building suites.

## Review round 2026-07-16b (max, full working tree) — all 15 findings fixed

Lockstep redesign (`sim::lockstep`): `submit` now covers EVERY owed tick through the input
horizon (`next_tick + delay`) — a catch-up burst back-fills instead of skipping keys (the
old `.max()` could deadlock every peer), and when frames outpace ticks empty frames send
NOTHING (bounded drift; only a command-bearing frame claims one extra tick). `pump` drops
messages from ids outside the roster (arrival-timing-dependent application was a silent
desync vector). XP integrity: the Repair WRECK lane pays only while progress accrues (a
rescue HELD at full progress mints nothing) and a rescuer standing ON the wreck tile now
fails loudly instead of holding forever; nest attacks pay Combat XP for damage DEALT
(a Defeated site at 0 hp is no longer an infinite farm). Q52: rescue/hijack boots filter
the color artifact against the chassis bars (over-bar → the inert fallback; note the
deploy layer already stock-caps REMAINDER artifacts, so hijack was closed at the source —
the live exposure was rescue-after-redeploy). Diplomacy: SetAlliance(false) strips grants
only when an alliance actually existed. Wreck race: `countdown_carry` re-arms when the
chassis is fully mended (docs/02: None = never wrecked since the last FULL window) — no
more one-way ratchet to insta-blast. Escalation: `ferals_killed` counts in settle_damage
where ATTRIBUTION is known — only non-Feral-attributed kills raise the footprint (docs/04:
fault-loops and blast chains are not player activity). `black_box` joined KINDS +
find_kind (recover_black_box() was unreachable from real programs). Feral deposits
re-check nest state at settle (a site beaten to Defeated mid-deposit absorbs nothing).
[game] view: wrecks get a retain/despawn pass (salvage/rescue removal is routine now);
black boxes are keyed by entity id instead of an append-only cursor, and recovered cubes
despawn. docs/02 updated to the docs/01 ruling: the scrap recall is the ECONOMY valve
(sustained Steel shortfall, `rust_scraps`); being over cap only stops prints. Regression
tests: 2 lockstep, 5 wreckrace, 3 ferals, 1 multiplayer. Golden unchanged (the fixture
exercises none of the touched paths).

## Spec-conformance review 2026-07-17 (max, M9–M13 vs docs) — 8 divergences fixed, 6 questions opened

Fixed where code contradicted a clear doc: **`attr`** now implemented in BotHost — entity property
reads (`t.distance`, docs/01) worked in no real game before (the trait method was never overridden,
so every read faulted); the fault carries the id so heard-only reads give err_unknown_contact.
**Hardware bars** (`analysis::artifact_requirements`) now derive from the parsed program — variable
slots count TOP-LEVEL names only (docs/01 Q80: def params/locals are frame-local), and program-memory
LINES count distinct statement-bearing lines (docstrings/comments/blanks/imports are not runtime code).
**comm_keys** hash gained a per-viewer length prefix (a missing one collided {1:{2},5:{6}} with
{1:{2,5,6}}, blinding the desync detector — ⚠HASH, golden regenerated: the length-prefix changes the
hash format unconditionally; the fixture has no comm keys, so this is format-only). **`study()`** faults
err_action "no Template Cache in range" instead of the misleading err_unknown_function the fallthrough
gave an advertised builtin. **Feral Calm** prints Drones only (docs/04's tutorial state), not Harvesters.
**Attack XP** on wrecks/structures is clamped to damage DEALT (matching the nest rule — an over-kill no
longer over-credits). **incoming_recolors** counts only queued recolors (color_population already counts
walkers — the sum double-counted). Regression tests: `tests/conformance.rs` (attr, study, comm-key hash),
`hardware_bar_counts_code_not_comments_or_frame_locals` (pyrite), feral-mix test updates.

Opened as design questions (docs/QUESTIONS.md Q86–Q91): lockstep command authorization (Q86 — cross-faction
commands trust their faction operand), nest→printer dormancy binding (Q87), the ruined-remainder-printer
ghost edge (Q88), faction ownership of depots + the archive/cloud (Q89), `try_receive` vs broadcasters
(Q90), alliance vs explicit harm (Q91). **ALL ANSWERED + IMPLEMENTED 2026-07-17** (QUESTIONS.md ruling;
sim/game changes; goldens regenerated for Q87/Q89's ⚠HASH state-format changes): **Q86** relay binds each
peer to its owned faction and drops mismatched commands at `try_step` (`Command::actor_faction` /
`Sim::command_actor_faction`; guards the relay, not the trusted golden log); **Q87** over-base printers bind
to their gating nest and go `PrinterState::Dormant` on its loss (`Sim::reconcile_dormancy`), closing the
nest-loss dormancy gap above; **Q88** the remainder printer can never be Ruined/Dormant (indestructible);
**Q89** Depots gained a `faction` field (see/hear for their owner) and the archive split into per-faction
clouds (`analyze()` files to the analyzer); **Q90** `try_receive` documented send-only (was already so);
**Q91** the alliance/explicit-harm split ratified as intended. The one remaining DECIDED-but-UNIMPLEMENTED
gap (not re-opened): **Template Caches** (Q79's `study()` has nothing to learn from — the whole
Cache/progression-learning system is unbuilt), follow-on milestone work flagged here so it isn't mistaken
for complete.

## Review round 2026-07-18 (high, M14 mapgen + game wiring) — all 8 findings fixed

Correctness: **seed variety was skin-deep** — the whole strategic skeleton (starts, veins, nests,
crystal, core) derived from geometry/player-count alone, so different seeds changed only the decorative
fill. Now the start ring is rotated by a seed-derived offset and per-wedge vein radii + nest arcanum +
the core's apex-nest/overlook diagonal are seed-jittered; the floor check was decoupled from geometry
(it reads the faction's remainder printer, its in-sight veins, its water, and its nearest Copper/Tin
from the SPEC — only band membership, which is seed-independent, still comes from geometry) so the
skeleton is free to vary. **Unbounded `MAPGEN_PLAYERS` panicked** (overcrowded rim → DuplicatePrinter or
an unsatisfiable floor, identical every retry): `generate` now clamps players to `max_supported_players`
(rim capacity at `max_size` / `MIN_START_SPACING`). **Guarantee tiles weren't bounds-clamped** like
`reserve` is: a new `Skeleton::place` reserves + routes + skips off-grid tiles, so a mis-tuned config
drops a guarantee (floor catches it → regenerate, now meaningful since the skeleton varies) instead of
emitting an OOB spec that panics world-build. **Malformed env vars swallowed silently**: `setup_sim`
now warns on a non-numeric `MAPGEN_SEED` (falls back to the showcase) or `MAPGEN_PLAYERS` (uses 1), and
`build_generated_colony` deploys to the factions actually seated (deterministic BTreeSet), not the raw
requested count. Cleanups: `paint_grid` is computed once per candidate (new `MapSpec::validate_grid`
returns the painted grid, shared by the authoring check and `floor_on_grid`); the sealed-start check is
folded into the flood (`Reach.non_rim`) instead of an O(players·size²) whole-grid rescan; the 4-neighbor
offset literal is one `NEIGHBORS4` const. Also folded the two verifier-refuted micro-nits (the `snow`
bounds loop into the array, the dead `let _ = kind`). +2 regression tests (seed varies the strategic
layout; huge player count clamps not panics). All sim + game suites green; golden replay unchanged
(mapgen never touches the tick or the state hash).

## Spec-conformance review 2026-07-19 (xhigh, full codebase vs docs) — 11 of 12 fixed

A whole-codebase audit against docs/00–09. **Income/XP over-credits** (all echoing already-hardened
sibling paths): Combat XP now pays for HP actually removed, not the full swing — the bot-attack and
guard/escort paths moved their per-damage credit into `settle_damage` (keyed on the attacker tag,
clamped to the real HP delta), and the Blight-Core path clamps inline like structures/nests/wrecks
(docs/02 "1 XP per 10 damage"). Same-tick **gank double-counts** fixed: the kill XP + first-kill Data
now sit under the `hp_before > 0` guard the escalation counter already carried, so two attackers dropping
one bot in a single settle mint ONE kill. **Hauling XP** is provenance-guarded (`credit_travel` accrues
only the mined share, `cargo_total − withdrawn_aboard`) so withdraw→lap→deposit farms nothing, matching
the Data-milestone rule. **Perception/API**: heard-only contacts expose `.distance` (from the blip, per
Q74) instead of faulting; `closest()`/`scan_*`/the `nearest_*` helpers rank by **Chebyshev** to match
`.distance` and the perception circles (was Manhattan); a multi-tick mover stays audible on its
in-between traverse ticks (`moved_tick` stamped every traverse tick, not only on tile change). **VM**:
`%` is checked like `//` (`i64::MIN % -1` faults err_overflow, no debug panic / release-0 split).
**Tuning-to-data**: High-Ground sensor bonus, Combat-L3 "+1 hearing vs enemies" (now implemented), and
guard/escort leash distances moved to `tuning.ron`. Regression tests added (combat overkill + gank,
hauling farm + mined control, multi-tick hearing, Mod overflow — the hearing and Mod tests verified to
fail without their fix). ⚠HASH (income/perception/closest feed phase 9; golden regenerated). ~~Not fixed:
Template Caches~~ — **now built (M15, 2026-07-20)**: the Cache entity, `study()`, per-match
function-block gating, and mapgen placement all landed.

## Test-coverage review 2026-07-20 (max, whole design) — 4 fixes + 11 coverage gaps closed

A max-effort audit of test coverage across docs/00–09 / M0–M15. **Correctness fixes**: the survey &
fog line-of-sight elevation flag now uses `on_high_ground` (Mountain summits see over walls too, not
just HighGround — `actions.rs` survey + `tile_visible`); `perceived()` no longer short-circuits black
boxes to always-seen (they're sight-gated like `find_kind`/`is_seen`, so `is_seen` agrees with
`exists`/`closest`); the decided **Scouting-L3 Corruption immunity** (Q75) is now implemented (an L3
scout skips the op-tax) and raced-tested; `env_read` clamps to the ENV_KEYS range so a quirk's
`EnvDefault` can't smuggle in an out-of-range value `setenv` would reject. **Coverage added** (tests
that fail if the mechanic breaks): the scouting stance (`search()` → node discovery + Scouting XP),
`scan_resources` distance-ordering, `is_seen`/`cargo_count`/`path_blocked` start-kit queries
(`tests/perception.rs`); `setenv`/`getenv` round-trip + range, `my_quirks`/`has_quirk` manifested-only
(`tests/env_quirks.rs`); a two-run state-hash guard exercising the `feral_mutation`/`wander`/`explore`/
`quirk_roll` RNG streams the golden never touches (`tests/determinism.rs`); Snow-mutes-movement + the
Scouting-L3 corruption race (`terrain.rs`); `try_receive`/`try_broadcast` success paths (`channels.rs`);
the Hiding XP track (`growth.rs`); the Crystal→Chips compute loop mined + refined from scratch
(`economy.rs`). All sim/pyrite/game suites green; golden unchanged (the fixes only alter previously
untested edges). *Not isolable under current tuning: the survey/passive elevation ranges coincide, so
finding [0]'s fix is defensive (guarded by the general survey test); the env-clamp fix has no shipped
out-of-range quirk to trigger it (guarded by the setenv-range test exercising the same path).*

