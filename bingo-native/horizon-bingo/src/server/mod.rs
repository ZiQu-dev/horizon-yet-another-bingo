//! Server half of "Yet Another Minecraft BINGO" for Horizon. Pure logic, no game types: everything the game gives or takes
//! goes through [`host::ServerHost`], so each module is tested without a server. `api/src/lib.rs` adapts Horizon to the trait.

pub mod host;
pub mod types;
pub mod card_gen;
pub mod pool;
pub mod game;
pub mod scoring;
pub mod wire;
pub mod items;
pub mod session;
pub mod commands;
pub mod ready;
pub mod teams;
pub mod options;
pub mod store;
pub mod hidden_consume;
pub mod multicard;
pub mod difficulty;
pub mod stats;
pub mod objectives;
pub mod map_render;
