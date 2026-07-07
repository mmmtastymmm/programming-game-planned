//! Deterministic world simulation. Renderer-free by rule
//! (docs/07-architecture.md); everything here obeys the determinism
//! contract in CLAUDE.md.
//!
//! - [`map`]: tile grid + deterministic A*
//! - [`world`]: entities, bots, wrecks, black boxes, the colony cloud
//! - [`host`]: the `pyrite::Host` implementation (builtins → world)
//! - [`sim`]: the fixed-tick phase loop and the `Command` input surface
//! - [`hash`]: FNV-1a state hashing for desync detection / golden replays

pub mod hash;
pub mod host;
pub mod map;
pub mod sim;
pub mod world;

pub use map::{MapSpec, TileKind, TilePos};
pub use sim::{Command, Sim};
pub use world::{BotId, EntityId, World};
