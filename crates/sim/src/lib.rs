//! Deterministic world simulation. Renderer-free by rule
//! (docs/07-architecture.md); everything here obeys the determinism
//! contract in CLAUDE.md.
//!
//! - [`map`]: tile grid + deterministic A*
//! - [`world`]: entities, bots, wrecks, black boxes, the colony cloud
//! - [`host`]: the `pyrite::Host` implementation (builtins → world)
//! - [`sim`]: the fixed-tick phase loop and the `Command` input surface
//! - [`hash`]: FNV-1a state hashing for desync detection / golden replays
//! - [`replay`]: the serialized `(map spec, command log)` artifact

mod actions;
mod damage;
pub mod hash;
mod movement;
mod printers;
pub mod host;
pub mod map;
pub mod replay;
pub mod resources;
pub mod sim;
pub mod stats;
pub mod world;

pub use map::{MapSpec, TileKind, TilePos};
pub use replay::{Replay, TimedCommand};
pub use sim::{Command, Sim};
pub use world::{BotId, EntityId, World};
