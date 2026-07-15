//! The typed resource catalog (docs/03): eleven raws → seven refined.
//! Every kind is a Pyrite constant of the same name; `ore` stays the
//! family constant for any mineral vein or seam. Quantities everywhere
//! (nodes, cargo, stock, prices) are DECI-UNITS (×10) so future
//! fractional yields (salvage cuts, regen trickles) stay integer.

use crate::map::TileKind;

/// One deci-unit scale factor: 1.0 display unit = 10 stored deci-units.
pub const DECI: u32 = 10;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
    serde::Serialize, serde::Deserialize,
)]
pub enum Resource {
    // --- the eleven raws ---
    Water,
    Stone,
    Sand,
    Wood,
    Coal,
    Iron,
    Copper,
    Tin,
    Silver,
    Gold,
    Crystal,
    // --- the seven refined ---
    Steel,
    Bronze,
    Wire,
    Chips,
    Glass,
    Lens,
    GoldChip,
}

impl Resource {
    pub const ALL: [Resource; 18] = [
        Resource::Water,
        Resource::Stone,
        Resource::Sand,
        Resource::Wood,
        Resource::Coal,
        Resource::Iron,
        Resource::Copper,
        Resource::Tin,
        Resource::Silver,
        Resource::Gold,
        Resource::Crystal,
        Resource::Steel,
        Resource::Bronze,
        Resource::Wire,
        Resource::Chips,
        Resource::Glass,
        Resource::Lens,
        Resource::GoldChip,
    ];

    /// The Pyrite constant / display name.
    pub fn name(self) -> &'static str {
        match self {
            Resource::Water => "water",
            Resource::Stone => "stone",
            Resource::Sand => "sand",
            Resource::Wood => "wood",
            Resource::Coal => "coal",
            Resource::Iron => "iron",
            Resource::Copper => "copper",
            Resource::Tin => "tin",
            Resource::Silver => "silver",
            Resource::Gold => "gold",
            Resource::Crystal => "crystal",
            Resource::Steel => "steel",
            Resource::Bronze => "bronze",
            Resource::Wire => "wire",
            Resource::Chips => "chips",
            Resource::Glass => "glass",
            Resource::Lens => "lens",
            Resource::GoldChip => "gold_chip",
        }
    }

    pub fn from_name(name: &str) -> Option<Resource> {
        Resource::ALL.into_iter().find(|r| r.name() == name)
    }

    /// Stable id for hashing.
    pub fn as_u8(self) -> u8 {
        Resource::ALL.iter().position(|r| *r == self).unwrap() as u8
    }

    pub fn is_raw(self) -> bool {
        (self as usize) <= (Resource::Crystal as usize)
    }

    /// Harvest tool tier (docs/03; enforcement lands with M5's tool
    /// modules — recorded now so the data is in place).
    pub fn tool_tier(self) -> Option<u8> {
        match self {
            Resource::Wood | Resource::Stone | Resource::Sand => Some(0),
            Resource::Iron | Resource::Coal => Some(1),
            Resource::Copper | Resource::Tin => Some(2),
            Resource::Silver | Resource::Gold => Some(3),
            Resource::Crystal => Some(4),
            Resource::Water => None, // pumped by a structure, not mined
            _ => None,               // refined goods aren't harvested
        }
    }

    /// Is this kind part of the `ore` family constant (any mineral vein
    /// or seam — docs/03: the starter program keeps working)?
    pub fn is_ore_family(self) -> bool {
        matches!(
            self,
            Resource::Coal
                | Resource::Iron
                | Resource::Copper
                | Resource::Tin
                | Resource::Silver
                | Resource::Gold
        )
    }

    /// The resource a node on this ground kind yields, with its
    /// regeneration flag (docs/03: regen is a per-node-type data flag;
    /// Wood groves are the flagship exception).
    pub fn for_tile(tile: TileKind) -> Option<(Resource, bool)> {
        match tile {
            TileKind::Sand => Some((Resource::Sand, false)),
            TileKind::StoneOutcrop => Some((Resource::Stone, false)),
            TileKind::Grove => Some((Resource::Wood, true)),
            TileKind::CoalSeam => Some((Resource::Coal, false)),
            TileKind::IronVein | TileKind::OreVein => Some((Resource::Iron, false)),
            TileKind::CopperVein => Some((Resource::Copper, false)),
            TileKind::TinVein => Some((Resource::Tin, false)),
            TileKind::SilverVein => Some((Resource::Silver, false)),
            TileKind::GoldVein => Some((Resource::Gold, false)),
            TileKind::CrystalField => Some((Resource::Crystal, false)),
            _ => None,
        }
    }
}

/// One refinery recipe (docs/03's tree): integer UNITS in the data file,
/// deci-converted at use. `station` says which structure kind runs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub name: &'static str,
    pub station: &'static str,
    pub inputs: &'static [(Resource, u32)],
    pub output: (Resource, u32),
}

/// The recipe book (docs/03; ratios are design-decided, batch time is
/// tuning). Index = the id `SetRecipe` and `Structure.recipe` store.
pub const RECIPES: &[Recipe] = &[
    Recipe { name: "steel", station: "smelter", inputs: &[(Resource::Iron, 2), (Resource::Coal, 1)], output: (Resource::Steel, 1) },
    Recipe { name: "bronze", station: "smelter", inputs: &[(Resource::Copper, 1), (Resource::Tin, 1)], output: (Resource::Bronze, 1) },
    Recipe { name: "glass", station: "smelter", inputs: &[(Resource::Sand, 2)], output: (Resource::Glass, 1) },
    Recipe { name: "wire", station: "foundry", inputs: &[(Resource::Copper, 1)], output: (Resource::Wire, 1) },
    Recipe { name: "chips", station: "foundry", inputs: &[(Resource::Silver, 1), (Resource::Crystal, 2), (Resource::Wire, 1)], output: (Resource::Chips, 1) },
    Recipe { name: "lens", station: "foundry", inputs: &[(Resource::Glass, 2)], output: (Resource::Lens, 1) },
    Recipe { name: "gold_chip", station: "foundry", inputs: &[(Resource::Chips, 1), (Resource::Gold, 1)], output: (Resource::GoldChip, 1) },
];
