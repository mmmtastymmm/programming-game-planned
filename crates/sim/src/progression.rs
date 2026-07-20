//! Per-match function-block progression (docs/06 *Template Caches* & the
//! Unlock Tree).
//!
//! Constructs (syntax) are permanent, account-scoped, Data-researched —
//! handled by [`pyrite::UnlockSet`]. This module is the OTHER axis: **function
//! blocks**, which gate *which builtins a colony's programs may call* this
//! match. They are LEARNED, not researched — a bot studies a Template Cache
//! ([`crate::world::Cache`]) with the start-kit `study()` verb, unlocking that
//! Cache's block colony-wide (non-consumable; allies and enemies can study the
//! same site). Depth replaces Data pricing: basic blocks ring each start,
//! richer ones sit deeper.
//!
//! A builtin not named in ANY block is ungated — always callable (the start
//! kit: `move_to`, `mine`, `deposit`, `study`, `closest`, `wait`, …). Gating
//! bites only when a program calls a *block* builtin whose block the colony
//! has not yet studied (and only on non-dev maps — dev sandboxes unlock all).

/// A learnable group of builtins, found at one Template Cache. The variant
/// order is the cache-depth order (docs/06's tree numbers): earlier = shallower
/// = rings the start zone.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum FunctionBlock {
    /// cargo_full, health_low, path_blocked — depth 5.
    Sense,
    /// log, upload_log, upload_crash_dump, recover_black_box, last_error — 10.
    Log,
    /// search, explore (the scouting stance) — depth 12.
    Search,
    /// attack — depth 15.
    Attack,
    /// salvage — depth 18.
    Salvage,
    /// build, repair — depth 20.
    Build,
    /// setenv, getenv — depth 25.
    Env,
    /// analyze — depth 30.
    Analyze,
    /// scan_enemies, scan_resources — depth 40.
    Scan,
    /// guard, escort — depth 45.
    Guard,
    /// hijack — depth 70.
    Hijack,
}

impl FunctionBlock {
    pub const ALL: [FunctionBlock; 11] = [
        FunctionBlock::Sense,
        FunctionBlock::Log,
        FunctionBlock::Search,
        FunctionBlock::Attack,
        FunctionBlock::Salvage,
        FunctionBlock::Build,
        FunctionBlock::Env,
        FunctionBlock::Analyze,
        FunctionBlock::Scan,
        FunctionBlock::Guard,
        FunctionBlock::Hijack,
    ];

    /// Stable id for hashing / serialization.
    pub fn as_u8(self) -> u8 {
        FunctionBlock::ALL.iter().position(|b| *b == self).unwrap() as u8
    }

    /// The builtins this block unlocks. Kept in sync with the registry by
    /// `progression_blocks_name_real_builtins` (tests) — every name here must
    /// be a real `builtins.ron` entry.
    pub fn builtins(self) -> &'static [&'static str] {
        match self {
            FunctionBlock::Sense => &["cargo_full", "health_low", "path_blocked"],
            FunctionBlock::Log => {
                &["log", "upload_log", "upload_crash_dump", "recover_black_box", "last_error"]
            }
            FunctionBlock::Search => &["search", "explore"],
            FunctionBlock::Attack => &["attack"],
            FunctionBlock::Salvage => &["salvage"],
            FunctionBlock::Build => &["build", "repair"],
            FunctionBlock::Env => &["setenv", "getenv"],
            FunctionBlock::Analyze => &["analyze"],
            FunctionBlock::Scan => &["scan_enemies", "scan_resources"],
            FunctionBlock::Guard => &["guard", "escort"],
            FunctionBlock::Hijack => &["hijack"],
        }
    }

    /// Cache depth (docs/06's tree numbers) — how far from a start the Cache
    /// holding this block sits. Data-ish, but structural to the tree, so it
    /// rides the block; mapgen turns it into a placement radius.
    pub fn depth(self) -> u32 {
        match self {
            FunctionBlock::Sense => 5,
            FunctionBlock::Log => 10,
            FunctionBlock::Search => 12,
            FunctionBlock::Attack => 15,
            FunctionBlock::Salvage => 18,
            FunctionBlock::Build => 20,
            FunctionBlock::Env => 25,
            FunctionBlock::Analyze => 30,
            FunctionBlock::Scan => 40,
            FunctionBlock::Guard => 45,
            FunctionBlock::Hijack => 70,
        }
    }

    /// Editor-facing name (docs/06 uses `F_*` tags).
    pub fn display_name(self) -> &'static str {
        match self {
            FunctionBlock::Sense => "F_SENSE",
            FunctionBlock::Log => "F_LOG",
            FunctionBlock::Search => "F_SEARCH",
            FunctionBlock::Attack => "F_ATK",
            FunctionBlock::Salvage => "F_SALV",
            FunctionBlock::Build => "F_BUILD",
            FunctionBlock::Env => "F_ENV",
            FunctionBlock::Analyze => "F_AN",
            FunctionBlock::Scan => "F_SCAN",
            FunctionBlock::Guard => "F_GUARD",
            FunctionBlock::Hijack => "F_HIJACK",
        }
    }
}

/// The block gating a builtin, or `None` if the builtin is ungated (always
/// callable — the start kit and everything not placed behind a Cache).
pub fn block_of(builtin: &str) -> Option<FunctionBlock> {
    FunctionBlock::ALL.into_iter().find(|b| b.builtins().contains(&builtin))
}
