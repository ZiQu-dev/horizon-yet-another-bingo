//! What the server logic needs from the game. The api adapter implements it over Horizon's `Host`; tests use a fake.
use super::types::Player;

pub trait ServerHost {
    /// Send a plugin message on `yet-another-minecraft-bingo:<path>` to one player. `path` is the channel path only.
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>);
    /// A chat line to one player, plain text.
    fn tell(&mut self, player: u128, line: &str);
    /// A chat line to everyone.
    fn broadcast(&mut self, line: &str);
    /// Players online right now.
    fn players(&self) -> Vec<Player>;
    /// The item's registry id on the wire (the `ItemStack.item` number), or `None` if the item does not exist.
    fn item_numeric_id(&self, item: &str) -> Option<i32>;
}
