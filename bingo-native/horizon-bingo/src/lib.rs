//! Client half of "Yet Another Minecraft BINGO" for Horizon (skeleton, see DESIGN.md).
//!
//! Layering mirrors the Kotlin mod: `buf` and the packet modules know nothing about the game,
//! `state` is what the client keeps between packets, and the drawing modules render it through
//! the `Host` trait. Only `Host` touches Horizon, so everything else is testable without a window.

pub mod channels;
pub mod host;
pub mod state;

// Verified against the real mod (tests/oracle.rs). New packets go beside them, one file each.
pub mod buf;
pub mod game;
pub mod ready;
pub mod scoring;
pub mod timer;
pub mod card_reset;
pub mod open_card;
pub mod card_shuffled;
pub mod game_status;
pub mod game_reset;
pub mod update_settings;
pub mod stats_check;
pub mod stats_game;
pub mod stats_index;
pub mod score_update;
pub mod card_image;
pub mod nbt;
pub mod text;
pub mod item;
pub mod card_tile;
pub mod card_tile_action;
pub mod card_tile_image;
pub mod update_card_tiles;
pub mod wire;
pub mod display_cards;
pub mod tooltip_v2;
pub mod ready_update;
pub mod game_over;
pub mod game_message;
pub mod client;
pub mod atlas;
pub mod text_plain;
pub mod card_draw;
pub mod assets;
pub mod message_draw;
pub mod ready_draw;
pub mod screen_end;
pub mod screen_cards;
pub mod screen_placement;
pub mod screen_main;
pub mod shuffle_draw;
pub mod tooltip_draw;
pub mod hud_tooltip;
pub mod ui_draw;
pub mod screen_controller;
pub mod scores_draw;
pub mod history_ctl;
pub mod card_images;
pub mod sounds;
pub mod fireworks;
pub mod config;
pub mod settings_screen;

pub mod hud_keys;
