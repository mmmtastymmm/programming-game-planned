//! Match plumbing (M13, docs/08): the settings inventory, Non-PvP harm
//! gating, diplomacy commands (data gifts, the Request Box, grants,
//! alliances), and unanimous sim-speed votes with cooldowns.

use sim::map::{HarmMode, MapSpec};
use sim::sim::{Command, Proposal, Sim};
use sim::world::{ArchiveKind, Color, GrantKind, FERAL_FACTION};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str, faction: u8) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 8,
        cargo_cap: 1,
        faction,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

fn logged(sim: &Sim, needle: &str) -> bool {
    sim.world
        .archive_all()
        .any(|e| e.kind == ArchiveKind::Log && e.text.contains(needle))
}

#[test]
fn match_settings_shadow_tuning_and_gate_the_ferals() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    spec.nests.push((TilePos::new(6, 2), 0));
    spec.settings.print_cost_steel = Some(77);
    spec.settings.salvage_decrypt_pct = Some(20);
    spec.settings.ferals = false;
    let sim = Sim::new(&spec);
    assert_eq!(sim.tuning.print_cost_steel, 77, "print cost is a match dial");
    assert_eq!(sim.tuning.salvage_decrypt_pct, 20, "decryption % is a match dial");
    assert!(sim.world.nests.is_empty(), "pure-PvP: the Ferals toggle empties the map");
    assert!(sim.world.harm_enabled, "Open is the default harm setting");
}

#[test]
fn non_pvp_blocks_player_harm_but_never_feral_hunts() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    spec.settings.harm = HarmMode::NonPvp;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let attacker_src = "\
if exists(enemy):
    attack(closest(enemy).expect())
wait(2)
";
    spawn(&mut sim, TilePos::new(1, 1), attacker_src, 0);
    // The Feral stands strictly closer than the rival, so closest(enemy)
    // hunts it first; once it falls, the rival becomes the target — and
    // the Non-PvP gate refuses every swing.
    let feral = spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n", FERAL_FACTION);
    let rival = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 1);
    for _ in 0..60 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&rival].data.hp, 100, "player-vs-player harm is refused");
    let feral_hurt = sim.world.bots.get(&feral).is_none_or(|b| b.data.hp < 100);
    assert!(feral_hurt, "Ferals are fair game on every server type");
    assert!(!sim.world.harm_allowed(0, 1) && sim.world.harm_allowed(0, FERAL_FACTION));
}

#[test]
fn data_gifts_clamp_and_the_request_box_caps() {
    let mut sim = Sim::new(&MapSpec::empty(6, 4));
    sim.world.data.insert(0, 50);
    sim.apply(&Command::ExchangeData { from: 0, to: 1, amount: 80 }).unwrap();
    assert_eq!(sim.world.data[&0], 0, "gifts clamp to what the giver has");
    assert_eq!(sim.world.data[&1], 50);

    let long = "x".repeat(500);
    sim.apply(&Command::PostRequest { faction: 1, text: long }).unwrap();
    assert_eq!(sim.world.requests.len(), 1);
    assert_eq!(sim.world.requests[0].2.len(), 200, "hostile text is clamped");
    for i in 0..70 {
        sim.apply(&Command::PostRequest { faction: 0, text: format!("r{i}") }).unwrap();
    }
    assert_eq!(sim.world.requests.len(), 64, "the board keeps the newest 64");
    assert_eq!(sim.world.requests.last().unwrap().2, "r69");
}

#[test]
fn vision_grants_pool_ears() {
    let mut spec = MapSpec::empty(16, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // Faction 0 has an eye next to a faction-2 target; faction 1 is far
    // across the map and blind to it.
    spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n", 0);
    let target = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 2);
    spawn(&mut sim, TilePos::new(14, 3), "wait(600)\n", 1);
    let entity = sim.world.bots[&target].data.entity;
    sim.step();
    assert!(
        !sim.world.perception.get(&1).is_some_and(|p| p.seen.contains(&entity)),
        "faction 1 can't see the far corner on its own"
    );
    sim.apply(&Command::Grant { from: 0, to: 1, what: GrantKind::Vision, revoke: false })
        .unwrap();
    sim.step();
    assert!(
        sim.world.perception[&1].seen.contains(&entity),
        "a Vision grant pools the granter's eyes"
    );
    sim.apply(&Command::Grant { from: 0, to: 1, what: GrantKind::Vision, revoke: true })
        .unwrap();
    sim.step();
    assert!(
        !sim.world.perception.get(&1).is_some_and(|p| p.seen.contains(&entity)),
        "revocation takes the eyes back"
    );
}

#[test]
fn channel_grants_open_the_namespace_without_a_stolen_key() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.apply(&Command::Grant { from: 1, to: 0, what: GrantKind::Channels, revoke: false })
        .unwrap();
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "x = receive(\"intel\", 60, 1)\nlog(\"granted\")\nupload_log()\nwait(600)\n",
        0,
    );
    spawn(&mut sim, TilePos::new(3, 1), "send(\"intel\", 3)\nwait(600)\n", 1);
    for _ in 0..60 {
        sim.step();
    }
    assert!(logged(&sim, "granted"), "the Channels grant substitutes for the comm key");
}

#[test]
fn allies_advance_decryption_together() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.apply(&Command::SetAlliance { a: 0, b: 1, allied: true }).unwrap();
    assert!(sim.world.allied(0, 1) && sim.world.allied(1, 0));
    // A faction-2 casualty; faction 0 salvages it with faction 1 allied.
    let victim = spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n", 2);
    spawn(
        &mut sim,
        TilePos::new(3, 1),
        "if exists(wreck):\n    salvage(closest(wreck).expect())\nwait(2)\n",
        0,
    );
    spawn(&mut sim, TilePos::new(8, 3), "wait(600)\n", 1); // the ally, idle
    sim.step();
    sim.apply(&Command::KillBot { bot: victim }).unwrap();
    for _ in 0..80 {
        sim.step();
    }
    let pct = sim.tuning.salvage_decrypt_pct;
    assert_eq!(
        sim.world.decryption.get(&(0, 2, Color::GREEN.0)).copied(),
        Some(pct),
        "the salvager reads"
    );
    assert_eq!(
        sim.world.decryption.get(&(1, 2, Color::GREEN.0)).copied(),
        Some(pct),
        "the declared ally advances with them (docs/08)"
    );
}

#[test]
fn sim_speed_votes_need_unanimity_and_respect_the_cooldown() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    spec.settings.vote_cooldown_ticks = 20;
    spec.settings.vote_window_ticks = 50;
    let mut sim = Sim::new(&spec);
    spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n", 0);
    spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 1);
    sim.step();

    let pause = Proposal::SetSpeed(0);
    sim.apply(&Command::Vote { faction: 0, proposal: pause, approve: true }).unwrap();
    assert_eq!(sim.world.sim_speed_permille, 1000, "one aye of two is not unanimity");
    assert!(sim.world.pending_vote.is_some());
    sim.apply(&Command::Vote { faction: 1, proposal: pause, approve: true }).unwrap();
    assert_eq!(sim.world.sim_speed_permille, 0, "unanimity applies the proposal");
    assert!(sim.world.pending_vote.is_none());

    // Cooldown: the immediate re-proposal is ignored.
    let resume = Proposal::SetSpeed(1000);
    sim.apply(&Command::Vote { faction: 0, proposal: resume, approve: true }).unwrap();
    assert!(sim.world.pending_vote.is_none(), "no vote spam inside the cooldown");
    for _ in 0..21 {
        sim.step();
    }
    sim.apply(&Command::Vote { faction: 0, proposal: resume, approve: true }).unwrap();
    assert!(sim.world.pending_vote.is_some(), "the cooldown elapsed");
    // A single refusal kills it.
    sim.apply(&Command::Vote { faction: 1, proposal: resume, approve: false }).unwrap();
    assert!(sim.world.pending_vote.is_none(), "one refusal fails a unanimity vote");
    assert_eq!(sim.world.sim_speed_permille, 0, "the refused proposal never applied");

    // Window expiry: an ignored proposal dies on its own — and still
    // starts the cooldown.
    for _ in 0..21 {
        sim.step();
    }
    sim.apply(&Command::Vote { faction: 0, proposal: resume, approve: true }).unwrap();
    assert!(sim.world.pending_vote.is_some());
    for _ in 0..51 {
        sim.step();
    }
    assert!(sim.world.pending_vote.is_none(), "unanswered proposals expire");
}

#[test]
fn post_request_clamps_on_char_boundaries() {
    let mut sim = Sim::new(&MapSpec::empty(6, 4));
    // 199 ASCII bytes then multi-byte chars straddling offset 200: the
    // clamp must land on a char boundary, never panic (review 2026-07-16).
    let hostile = format!("{}🦀🦀🦀", "x".repeat(199));
    sim.apply(&Command::PostRequest { faction: 0, text: hostile }).unwrap();
    let posted = &sim.world.requests[0].2;
    assert!(posted.len() <= 200, "clamped");
    assert_eq!(posted.chars().filter(|c| *c == 'x').count(), 199, "the ASCII prefix survives");
}

#[test]
fn guard_swings_respect_harm_and_alliances() {
    // Non-PvP: a guard's autonomous swing obeys the same gate as attack().
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    spec.settings.harm = HarmMode::NonPvp;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let src = "x = closest(enemy).expect()\nguard(x)\nwait(600)\n";
    spawn(&mut sim, TilePos::new(2, 1), src, 0);
    let rival = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 1);
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&rival].data.hp, 100, "Non-PvP: the guard never swings at players");

    // Open server, declared allies: the guard holds its swing too.
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.apply(&Command::SetAlliance { a: 0, b: 1, allied: true }).unwrap();
    spawn(&mut sim, TilePos::new(2, 1), src, 0);
    let ally = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 1);
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&ally].data.hp, 100, "declared allies are never auto-attacked");
}

#[test]
fn non_pvp_protects_claimed_nests() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    spec.settings.harm = HarmMode::NonPvp;
    spec.nests.push((TilePos::new(6, 2), 0));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.tuning.nest_print_ticks = 10_000;
    let nid = *sim.world.nests.keys().next().unwrap();
    sim.world.nests.get_mut(&nid).unwrap().state =
        sim::world::NestState::Claimed(0);
    let hp = sim.world.nests[&nid].hp;
    // A rival stands adjacent and hammers the claimed site.
    spawn(
        &mut sim,
        TilePos::new(5, 2),
        "if exists(nest):\n    attack(closest(nest).expect())\nwait(1)\n",
        1,
    );
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.nests[&nid].hp, hp, "a claimed nest is player property: harm refused");
    assert_eq!(sim.world.nests[&nid].state, sim::world::NestState::Claimed(0));
}

#[test]
fn vision_grants_never_chain_transitively() {
    let mut spec = MapSpec::empty(16, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // Faction 0's eye alone can see the target; 1 and 2 sit far away.
    spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n", 0);
    let target = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n", 3);
    spawn(&mut sim, TilePos::new(14, 1), "wait(600)\n", 1);
    spawn(&mut sim, TilePos::new(14, 3), "wait(600)\n", 2);
    let entity = sim.world.bots[&target].data.entity;
    sim.apply(&Command::Grant { from: 0, to: 1, what: GrantKind::Vision, revoke: false })
        .unwrap();
    sim.apply(&Command::Grant { from: 1, to: 2, what: GrantKind::Vision, revoke: false })
        .unwrap();
    sim.step();
    assert!(sim.world.perception[&1].seen.contains(&entity), "0→1 pools 0's eyes");
    assert!(
        !sim.world.perception[&2].seen.contains(&entity),
        "1→2 hands over 1's OWN eyes only — grants never chain (review 2026-07-16)"
    );
}

#[test]
fn declining_a_nonexistent_alliance_keeps_grants() {
    let mut sim = Sim::new(&MapSpec::empty(6, 4));
    sim.apply(&Command::Grant { from: 0, to: 1, what: GrantKind::Vision, revoke: false })
        .unwrap();
    // No alliance exists: a "dissolve" (declining an offer) must not
    // strip independently issued grants (review 2026-07-16).
    sim.apply(&Command::SetAlliance { a: 0, b: 1, allied: false }).unwrap();
    assert!(
        sim.world.granted(0, 1, GrantKind::Vision),
        "no alliance was broken, so no grants are stripped"
    );
    // A REAL break still takes its grants with it.
    sim.apply(&Command::SetAlliance { a: 0, b: 1, allied: true }).unwrap();
    sim.apply(&Command::SetAlliance { a: 0, b: 1, allied: false }).unwrap();
    assert!(
        !sim.world.granted(0, 1, GrantKind::Vision),
        "a broken alliance strips the pair's grants"
    );
}
