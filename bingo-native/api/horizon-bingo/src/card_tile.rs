//! `CardTile.V1` (channel `card_tile`). Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardTile.kt.
//! The transient fields of the Kotlin class (`updatedAt`, `clientTooltip`, `mapRenderKey`) and the derived
//! `isFlashingOnMap` are not on the wire, so they are not modelled.

use crate::card_tile_action::CardTileAction;
use crate::card_tile_image::CardTileImage;
use crate::wire::{read_list, write_list};
use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::text::{read_text, write_text, Text};

/// Channel id of the `CardTile.V1` converter.
pub const ID: &str = "yet-another-minecraft-bingo:card_tile";

/// `TierLabel`, written with `writeNullable(name, writeString)` and read with `TierLabel.valueOf(readString())`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierLabel {
    S,
    A,
    B,
    C,
    D,
}

impl TierLabel {
    pub fn name(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
        }
    }

    /// `valueOf`: an exact name, nothing else.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::S, Self::A, Self::B, Self::C, Self::D].into_iter().find(|tier| tier.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardTile {
    /// Written as the string, or an empty string for `None` (the Kotlin side does `orEmpty()`, and reads "" as `None`).
    pub id: Option<String>,
    pub image: CardTileImage,
    pub image_list: Vec<CardTileImage>,
    pub action: CardTileAction,
    pub item_tier: Option<TierLabel>,
    pub name: Option<Text>,
    pub lore: Vec<Text>,
    pub is_hidden: bool,
    pub is_locked: bool,
    /// Kotlin `Boolean?`, written as `isFlashing ?: false`, so a read always gives a value.
    pub is_flashing: bool,
    pub progress: f32,
    /// `BingoTeamKey.id` strings.
    pub team_keys: Vec<String>,
}

impl CardTile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.read_string()?;
        let id = (!id.is_empty()).then_some(id);
        let image = CardTileImage::read(r)?;
        let image_list = read_list(r, CardTileImage::read)?;
        let action = CardTileAction::read(r)?;
        let item_tier = r.read_nullable(|r| {
            let name = r.read_string()?;
            TierLabel::from_name(&name).ok_or(DecodeError::Unsupported("CardTile.itemTier (not a tier label name)"))
        })?;
        let name = r.read_nullable(read_text)?;
        let lore = read_list(r, read_text)?;
        let is_hidden = r.read_bool()?;
        let is_locked = r.read_bool()?;
        let is_flashing = r.read_bool()?;
        let progress = r.read_f32()?;
        let team_keys = read_list(r, |r| r.read_string())?;
        Ok(Self {
            id,
            image,
            image_list,
            action,
            item_tier,
            name,
            lore,
            is_hidden,
            is_locked,
            is_flashing,
            progress,
            team_keys,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.write_string(self.id.as_deref().unwrap_or(""));
        self.image.write(w)?;
        write_list(w, &self.image_list, |w, image| image.write(w))?;
        self.action.write(w)?;
        w.write_nullable(self.item_tier.as_ref(), |w, tier| w.write_string(tier.name()));
        w.write_bool(self.name.is_some());
        if let Some(name) = &self.name {
            write_text(w, name)?;
        }
        write_list(w, &self.lore, write_text)?;
        w.write_bool(self.is_hidden);
        w.write_bool(self.is_locked);
        w.write_bool(self.is_flashing);
        w.write_f32(self.progress);
        write_list(w, &self.team_keys, |w, key| {
            w.write_string(key);
            Ok(())
        })?;
        Ok(())
    }
}
