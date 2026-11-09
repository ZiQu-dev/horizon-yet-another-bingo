//! `CardTilesPacket.V1` (channel `update_card_tiles`). Mirrors
//! common/src/main/java/me/jfenn/bingo/common/map/CardTilesPacket.kt.
//!
//! The Kotlin `Map<Int, CardTile>` is written in its iteration order, one (INT index, CardTile) entry after another.
//! It is kept here as a `Vec` of pairs in wire order, so duplicate indices and order survive a round trip.

use crate::card_tile::CardTile;
use crate::wire::{read_list, write_list};
use crate::buf::{Reader, Result, Writer};

#[derive(Debug, Clone, PartialEq)]
pub struct CardTilesPacket {
    /// `BingoTeamKey.id` (for example `bingo_red`), or `None` for no team.
    pub team_key: Option<String>,
    pub tiles: Vec<(i32, CardTile)>,
    pub should_notify: bool,
}

impl CardTilesPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:update_card_tiles";

    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let team_key = r.read_nullable(|r| r.read_string())?;
        let tiles = read_list(r, |r| {
            let index = r.read_i32()?;
            let tile = CardTile::read(r)?;
            Ok((index, tile))
        })?;
        let should_notify = r.read_bool()?;
        Ok(Self { team_key, tiles, should_notify })
    }

    /// Like [`Self::read`], but a tile that cannot be read (a data component this crate does not know has no length to
    /// skip it by) ends the list instead of failing the whole packet: the tiles before it are returned with the error.
    /// `should_notify` is unknown then and is false, so a partly known card does not flash.
    pub fn read_salvaging(r: &mut Reader<'_>) -> Result<(Self, Option<crate::buf::DecodeError>)> {
        let team_key = r.read_nullable(|r| r.read_string())?;
        let count = r.read_i32()?.max(0);
        let mut tiles = Vec::new();
        for _ in 0..count {
            let one = (|| -> Result<(i32, CardTile)> {
                let index = r.read_i32()?;
                Ok((index, CardTile::read(r)?))
            })();
            match one {
                Ok(tile) => tiles.push(tile),
                Err(e) => return Ok((Self { team_key, tiles, should_notify: false }, Some(e))),
            }
        }
        let should_notify = r.read_bool()?;
        Ok((Self { team_key, tiles, should_notify }, None))
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.write_bool(self.team_key.is_some());
        if let Some(key) = &self.team_key {
            w.write_string(key);
        }
        write_list(w, &self.tiles, |w, (index, tile)| {
            w.write_i32(*index);
            tile.write(w)
        })?;
        w.write_bool(self.should_notify);
        Ok(())
    }
}
