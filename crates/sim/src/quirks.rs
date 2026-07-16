//! Quirks (M6, docs/09): small per-bot deviations from the universal
//! chassis, rolled LATENT at print from the seeded `quirk_roll` stream
//! and manifesting only when total XP crosses a threshold (300/900 —
//! kill-and-reprint reroll-fishing buys nothing). Data-driven
//! (`data/quirks.ron`); the v1 catalog carries only effects whose hooks
//! exist — per-bot COST-TABLE overlays (Tail-Call Optimized, Kernel
//! Bypass, …) wait for M8's overlay machinery, flagged in TASKS.md.

use crate::world::BotData;

/// The quirk catalog + thresholds (loaded from `data/quirks.ron`).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuirkCatalog {
    /// Total-XP thresholds (whole XP) at which the nth latent quirk
    /// manifests (docs/09 first-pass: 300, 900). Length = max latent
    /// rolls per print.
    pub manifest_at: Vec<u64>,
    pub quirks: Vec<QuirkSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuirkSpec {
    /// The pre-bound constant name (`has_quirk(overclocked)`).
    pub name: String,
    /// Rarity weight for the print roll (higher = more common).
    pub weight: u32,
    /// Stat/timing effects, applied in list order by the pipeline (or
    /// once, at manifestation, where the note says so).
    pub effects: Vec<QuirkEffect>,
}

/// One quirk effect. Percent values are integer percents of the running
/// subtotal (pessimistic rounding, like every pipeline layer).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum QuirkEffect {
    /// ± centicycles per tick (Overclocked +100, `unsafe` Block +200…).
    CpuCenti(i64),
    /// ± VM stack frames (applied to the live VM at manifestation).
    StackDepth(i32),
    /// ± sensor tiles.
    Sensors(i32),
    /// ±% cargo capacity.
    CargoPct(i32),
    /// ±% max HP (one-time, at manifestation).
    MaxHpPct(i32),
    /// ±% XP earned, all tracks (joins the Learning multiplier).
    XpPct(i32),
    /// ± flinch ticks (`handler_init` duration).
    FlinchTicks(i32),
    /// ±% boot ritual duration.
    BootPct(i32),
    /// ±% unhandled-fault chip damage.
    FaultChipPct(i32),
    /// ±% attack damage dealt.
    DamagePct(i32),
    /// ±% move rate (rate is ticks/tile — negative is faster).
    MovePct(i32),
    /// Brownout costs this percent instead of the stats.ron figure
    /// (Energy Star: 25).
    BrownoutPenaltyPct(u32),
    /// ±% log ring-buffer cap (one-time, at manifestation; min 1).
    LogCapPct(i32),
    /// TEMPERAMENT (docs/09 Q60): this env key's DEFAULT shifts — programs
    /// that never touch the key inherit the personality; one setenv
    /// overrides it entirely.
    EnvDefault { key: EnvKeyName, value: i64 },
    /// COMPULSION (docs/09 Q60): the key's legal range narrows; setenv
    /// past the clamp CLIPS quietly and getenv reports where it landed.
    EnvClamp { key: EnvKeyName, min: i64, max: i64 },
}

/// Env keys addressable from quirk data (a closed set so quirks.ron can't
/// name a key the registry doesn't have).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum EnvKeyName {
    HurtLine,
    LogMinLevel,
}

impl EnvKeyName {
    pub fn as_str(self) -> &'static str {
        match self {
            EnvKeyName::HurtLine => "hurt_line",
            EnvKeyName::LogMinLevel => "log_min_level",
        }
    }
}

impl Default for QuirkCatalog {
    fn default() -> Self {
        let cat: QuirkCatalog = ron::from_str(include_str!("../data/quirks.ron"))
            .expect("data/quirks.ron parses (unknown fields are errors)");
        assert!(!cat.quirks.is_empty(), "quirks: catalog must not be empty");
        assert!(cat.quirks.len() <= u8::MAX as usize, "quirks: indices are u8");
        assert!(
            cat.quirks.iter().all(|q| q.weight > 0),
            "quirks: zero-weight entries can never roll"
        );
        let mut names: Vec<&str> = cat.quirks.iter().map(|q| q.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), cat.quirks.len(), "quirks: names must be unique");
        cat
    }
}

impl QuirkCatalog {
    pub fn by_name(&self, name: &str) -> Option<u8> {
        self.quirks.iter().position(|q| q.name == name).map(|i| i as u8)
    }

    /// Iterate a bot's MANIFESTED quirk effects in manifestation order
    /// (latent quirks do not exist to the world — docs/09).
    pub fn effects_of<'a>(
        &'a self,
        data: &'a BotData,
    ) -> impl Iterator<Item = QuirkEffect> + 'a {
        data.quirks
            .iter()
            .filter_map(move |&q| self.quirks.get(q as usize))
            .flat_map(|spec| spec.effects.iter().copied())
    }

    /// Weighted pick from the catalog for one latent slot. `r` is a raw
    /// draw from the `quirk_roll` stream.
    pub fn pick(&self, r: u64) -> u8 {
        let total: u64 = self.quirks.iter().map(|q| q.weight as u64).sum();
        let mut ticket = r % total;
        for (i, q) in self.quirks.iter().enumerate() {
            if ticket < q.weight as u64 {
                return i as u8;
            }
            ticket -= q.weight as u64;
        }
        (self.quirks.len() - 1) as u8
    }
}
