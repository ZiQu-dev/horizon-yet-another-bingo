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
    /// A console command (for example `team add bingo_red`), queued and run at the next server tick. Commands
    /// that fail are only logged by the game. The default does nothing, so a test host need not implement it.
    fn command(&mut self, _line: &str) {}
    /// Shows or updates a boss bar for one player. `progress` is 0.0 to 1.0. `visible` false removes it.
    fn boss_bar(&mut self, _player: u128, _id: &str, _title: &str, _progress: f32, _visible: bool) {}
    /// Reads a file of the mod's data folder (for example `options.json`), or `None` if it is missing. The default
    /// has no files, so a test host need not implement it.
    fn read_file(&mut self, _name: &str) -> Option<String> {
        None
    }
    /// Replaces a file of the mod's data folder.
    fn write_file(&mut self, _name: &str, _content: &str) {}
    /// Appends one line (the `\n` is added) to a file of the mod's data folder.
    fn append_line(&mut self, _name: &str, _line: &str) {}
    /// Wall-clock time in milliseconds since the Unix epoch, for the history. The default is 0.
    fn now_ms(&self) -> i64 {
        0
    }
}
