//! Construct gating (docs/01-language.md "Syntax by Tier", docs/06-progression.md).
//!
//! Constructs are *permanent account unlocks*; the parser takes an `UnlockSet`
//! and rejects locked syntax with a structured `LockedConstruct` error the
//! editor can render as "requires <unlock>".

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Construct {
    /// Tier 1: variables & arithmetic (assignment).
    Variables,
    /// Tier 2: `if` / `elif` / `else`.
    If,
    /// Tier 3: `while`, `break`, `continue`.
    WhileLoop,
    /// Tier 4: `def` / `return` (user functions; recursion under stack cap).
    Functions,
    /// Tier 5: list literals, indexing, and `for x in xs`.
    Lists,
    /// Tier 6: `enum` declarations + `match`.
    Enums,
    /// `import m` / `from m import f` — its own construct, right after
    /// `def` (Q61): learn to write functions before you share them.
    Import,
    /// `on error:` window.
    OnError,
    /// `on hurt:` window.
    OnHurt,
    /// `on bump:` + `on bumped:` windows (one unlock per docs/06's tree).
    OnBumpBumped,
    /// `on boot:` window — the bot's dotfile.
    OnBoot,
    /// `send` / `receive` channel syntax (verbs land in M11).
    Channels,
}

impl Construct {
    pub fn display_name(self) -> &'static str {
        match self {
            Construct::Variables => "Variables",
            Construct::If => "if / elif / else",
            Construct::WhileLoop => "while / break / continue",
            Construct::Functions => "def / return",
            Construct::Lists => "containers (lists, dicts) + for-in",
            Construct::Enums => "enum + match",
            Construct::Import => "import / from-import",
            Construct::OnError => "on error: window",
            Construct::OnHurt => "on hurt: window",
            Construct::OnBumpBumped => "on bump: / on bumped: windows",
            Construct::OnBoot => "on boot: window",
            Construct::Channels => "channels: send / receive",
        }
    }

    pub const ALL: [Construct; 12] = [
        Construct::Variables,
        Construct::If,
        Construct::WhileLoop,
        Construct::Functions,
        Construct::Lists,
        Construct::Enums,
        Construct::Import,
        Construct::OnError,
        Construct::OnHurt,
        Construct::OnBumpBumped,
        Construct::OnBoot,
        Construct::Channels,
    ];
}

/// The set of constructs a colony's programs may use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnlockSet {
    unlocked: BTreeSet<Construct>,
}

impl UnlockSet {
    /// Tier 0: straight-line programs only.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        let mut set = Self::default();
        for c in Construct::ALL {
            set.unlocked.insert(c);
        }
        set
    }

    pub fn with(mut self, construct: Construct) -> Self {
        self.unlocked.insert(construct);
        self
    }

    pub fn unlock(&mut self, construct: Construct) {
        self.unlocked.insert(construct);
    }

    pub fn has(&self, construct: Construct) -> bool {
        self.unlocked.contains(&construct)
    }
}
