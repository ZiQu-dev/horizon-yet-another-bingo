//! What the server logic needs from the game. The api adapter implements it over Horizon's `Host`; tests use a fake.
use super::map_render::DirtyRect;
use super::texts::Line;
use super::types::Player;

pub trait ServerHost {
    /// Send a plugin message on `yet-another-minecraft-bingo:<path>` to one player. `path` is the channel path only.
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>);
    /// A chat line to one player, plain text.
    fn tell(&mut self, player: u128, line: &str);
    /// A chat line to everyone.
    fn broadcast(&mut self, line: &str);
    /// A translatable chat line to one player. The default sends its English, so a test host need not implement it.
    fn tell_line(&mut self, player: u128, line: &Line) {
        self.tell(player, &line.render());
    }
    /// A translatable chat line to everyone. The default sends its English.
    fn broadcast_line(&mut self, line: &Line) {
        self.broadcast(&line.render());
    }
    /// Players online right now.
    fn players(&self) -> Vec<Player>;
    /// The item's registry id on the wire (the `ItemStack.item` number), or `None` if the item does not exist.
    fn item_numeric_id(&self, item: &str) -> Option<i32>;
    /// A console command (for example `team add bingo_red`), queued and run at the next server tick. Commands
    /// that fail are only logged by the game. The default does nothing, so a test host need not implement it.
    fn command(&mut self, _line: &str) {}
    /// Shows or updates a boss bar for one player. `progress` is 0.0 to 1.0. `visible` false removes it.
    fn boss_bar(&mut self, _player: u128, _id: &str, _title: &str, _progress: f32, _visible: bool) {}
    /// A title and a subtitle in the middle of one player's screen (`/title`), with the default times. The default does
    /// nothing, so a test host need not implement it.
    fn title(&mut self, _player: u128, _title: &Line, _subtitle: &Line) {}
    /// Reads a file of the mod's data folder (for example `options.json`), or `None` if it is missing. The default
    /// has no files, so a test host need not implement it.
    fn read_file(&mut self, _name: &str) -> Option<String> {
        None
    }
    /// Replaces a file of the mod's data folder.
    fn write_file(&mut self, _name: &str, _content: &str) {}
    /// Logs one warning line for the operator (not a player). The default logs nothing.
    fn warn(&mut self, _line: &str) {}
    /// Appends one line (the `\n` is added) to a file of the mod's data folder.
    fn append_line(&mut self, _name: &str, _line: &str) {}
    /// Wall-clock time in milliseconds since the Unix epoch, for the history. The default is 0.
    fn now_ms(&self) -> i64 {
        0
    }
    /// A statistic's counter for a player (`stat_type` such as `minecraft:mined`, `stat_name` such as `minecraft:stone`),
    /// or `None` when the host cannot read it. The default is `None`, so a test host need not implement it.
    fn stat_value(&self, _player: u128, _stat_type: &str, _stat_name: Option<&str>) -> Option<i64> {
        None
    }
    /// Whether a player has the advancement done (`minecraft:story/root`). The default is `false`.
    fn has_advancement(&self, _player: u128, _id: &str) -> bool {
        false
    }
    /// A new map of the overworld showing `pixels` (16 384 map colour ids, row by row), kept by the world, under an id
    /// that stays for the server's life. `None` when the host cannot make one. The default is `None`.
    fn new_map(&mut self, _pixels: &[u8]) -> Option<i32> {
        None
    }
    /// Replaces the whole picture of a map made by `new_map`, for everyone carrying it. `dirty` is the part that changed;
    /// a host that can only send the whole picture may ignore it. False when the map is not there. The default is false.
    fn paint_map(&mut self, _map_id: i32, _pixels: &[u8], _dirty: DirtyRect) -> bool {
        false
    }
    /// Sends a map's picture to one player, for a map they hold. False when the player is not here. The default is false.
    fn send_map(&mut self, _player: u128, _map_id: i32, _pixels: &[u8]) -> bool {
        false
    }
    /// Gives one item, from its SNBT text (`{id:"minecraft:filled_map",count:1,components:{...}}`). False when it could
    /// not be given. The default is false.
    fn give_map(&mut self, _player: u128, _snbt: &str) -> bool {
        false
    }
    /// Whether a player has the bingo operator level (`bingo.op`), which the lobby's book and menu read. The default is
    /// `false`.
    fn is_operator(&self, _player: u128) -> bool {
        false
    }
    /// Gives one item of a kind with its components, in the `give` command's syntax (`minecraft:written_book[...]`),
    /// for example the tutorial book (`infobook`). The adapter runs it as a `give` command, so a `true` means the player
    /// was online to get it, not that the item is already in their inventory. The default is false.
    fn give_item(&mut self, _player: u128, _item: &str, _components: &str) -> bool {
        false
    }
}
