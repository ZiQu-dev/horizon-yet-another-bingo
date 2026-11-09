//! The client driver against a recording fake host: channel registration, re-registration on a new join, sounds,
//! errors, and the overlay it draws from the real server's packets.
use horizon_bingo::buf::encode;
use horizon_bingo::channels::id;
use horizon_bingo::client::{register_payload, BingoClient, ScreenKind, ANNOUNCE_FOR_TESTS, REGISTER_CHANNEL, TICK_MS};
use horizon_bingo::host::{Argb, Host};
use horizon_bingo::state::SoundKey;
use horizon_bingo::update_settings::PlayerSettings;

#[derive(Default)]
struct Fake {
    /// While true, sends fail as they do before the player has joined.
    offline: bool,
    sent: Vec<(String, Vec<u8>)>,
    rects: Vec<(i32, i32, i32, i32, Argb)>,
    texts: Vec<(i32, i32, String, u32, bool)>,
    /// `draw_text_xf` calls: text, anchor, offset, scale, rotation, colour, shadow.
    xf: Vec<(String, (f32, f32), (f32, f32), f32, f32, Argb, bool)>,
    uploads: Vec<(u32, u32)>,
    sprites: Vec<([f32; 4], [f32; 4])>,
    items: Vec<(String, f32, f32, f32)>,
    image_ok: bool,
    gui_width: Option<i32>,
    vanilla: Vec<String>,
    tooltips: Vec<Vec<String>>,
    gui: Option<(i32, i32)>,
    playing: bool,
    keys: Vec<String>,
    screen_up: bool,
    opens: u32,
    closes: u32,
    sounds: Vec<(SoundKey, f32, f32)>,
    /// The heads the client asked for, and whether the skin is ready to give.
    face_calls: Vec<u128>,
    face_ready: bool,
    /// The number of draws made when `begin_alpha` was called, and (draws so far, alpha) for each `end_alpha`.
    fade_begins: Vec<usize>,
    fade_ends: Vec<(usize, f32)>,
    /// Whether each card picture (a 122 x 132 sprite) was drawn inside a fade, in the order they were drawn.
    card_faded: Vec<bool>,
    in_fade: bool,
    /// Sneak, the chat screen, the F1 hide, F3's debug screen and the Tab list key, as the host reports them.
    sneak: bool,
    chat: bool,
    f1_hidden: bool,
    f3: bool,
    tab: bool,
    /// The mod's settings as the host keeps them (`setting` and `set_setting`).
    settings: std::collections::BTreeMap<String, String>,
    /// The configs the host was told (`config_changed`), and the test sounds it played (`play_test_sound`).
    configs: Vec<String>,
    test_sounds: Vec<(String, f32)>,
    /// The player under the crosshair, as `looked_at_player` reports it.
    look: Option<u128>,
}

impl Host for Fake {
    fn send_to_server(&mut self, channel: &str, data: &[u8]) -> Result<(), String> {
        if self.offline {
            return Err("the player has not joined yet".into());
        }
        self.sent.push((channel.to_string(), data.to_vec()));
        Ok(())
    }
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb) {
        self.rects.push((x, y, w, h, color));
    }
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool) {
        self.texts.push((x, y, text.to_string(), color, shadow));
    }
    fn draw_text_xf(&mut self, text: &str, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, color: Argb, shadow: bool) {
        self.xf.push((text.to_string(), anchor, offset, scale, rotation, color, shadow));
    }
    fn gui_width(&self) -> Option<i32> {
        self.gui_width
    }
    fn begin_alpha(&mut self) -> usize {
        self.fade_begins.push(self.draws());
        self.in_fade = true;
        self.draws()
    }
    fn end_alpha(&mut self, _start: usize, alpha: f32) {
        self.fade_ends.push((self.draws(), alpha));
        self.in_fade = false;
    }
    fn upload_image(&mut self, width: u32, height: u32, _: &[u8]) -> bool {
        self.uploads.push((width, height));
        self.image_ok
    }
    fn player_face(&mut self, uuid: u128) -> Option<Vec<u8>> {
        self.face_calls.push(uuid);
        self.face_ready.then(|| vec![200; 8 * 8 * 4])
    }
    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]) {
        self.sprites.push((src, dst));
        if dst[2] == 122.0 && dst[3] == 132.0 {
            self.card_faded.push(self.in_fade);
        }
    }
    fn gui_size(&self) -> Option<(i32, i32)> {
        self.gui
    }
    fn cursor(&self) -> (i32, i32) {
        (0, 0)
    }
    fn is_playing_screen(&self) -> bool {
        self.playing
    }
    fn key_down(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }
    fn open_screen(&mut self) -> bool {
        self.opens += 1;
        self.screen_up = true;
        true
    }
    fn close_screen(&mut self) {
        self.closes += 1;
        self.screen_up = false;
    }
    fn screen_is_open(&self) -> bool {
        self.screen_up
    }
    fn draw_vanilla_nine_slice(&mut self, sprite: &str, _x: f32, _y: f32, _w: f32, _h: f32, _argb: Argb) {
        self.vanilla.push(sprite.to_string());
    }
    fn draw_tooltip(&mut self, lines: &[String], _x: i32, _y: i32) {
        self.tooltips.push(lines.to_vec());
    }
    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32) {
        self.items.push((name.to_string(), x, y, size));
    }
    fn text_width(&self, text: &str) -> i32 {
        6 * text.chars().count() as i32
    }
    fn play_sound(&mut self, key: SoundKey, volume: f32, pitch: f32) {
        self.sounds.push((key, volume, pitch));
    }
    fn sneaking(&self) -> bool {
        self.sneak
    }
    fn chat_open(&self) -> bool {
        self.chat
    }
    fn hud_hidden(&self) -> bool {
        self.f1_hidden
    }
    fn debug_screen(&self) -> bool {
        self.f3
    }
    fn tab_list_down(&self) -> bool {
        self.tab
    }
    fn setting(&self, key: &str) -> Option<String> {
        self.settings.get(key).cloned()
    }
    fn set_setting(&mut self, key: &str, value: &str) {
        self.settings.insert(key.to_string(), value.to_string());
    }
    fn config_changed(&mut self, json: &str) {
        self.configs.push(json.to_string());
    }
    fn play_test_sound(&mut self, id: &str, volume: f32) {
        self.test_sounds.push((id.to_string(), volume));
    }
    fn looked_at_player(&self) -> Option<u128> {
        self.look
    }
}

/// The 24 channels the server sends, as the mod's `ServerPacketEvents.kt:23-55` registers them (the audit's table).
const SERVER_CHANNELS: [&str; 24] = [
    "card_reset", "display_cards", "update_card_tiles", "card_image", "open_card", "card_shuffled", "card_completed",
    "game_over_v2", "game_over", "score_update", "scored_item_lost", "game_status", "game_message", "game_message_clear",
    "timer", "countdown", "tooltip_v2", "game_reset", "ready_timer_update_v2", "ready_timer_update", "update_settings",
    "stats_check", "stats_game", "stats_index",
];

fn registers(host: &Fake) -> usize {
    host.sent.iter().filter(|(c, _)| c == REGISTER_CHANNEL).count()
}

#[test]
fn it_registers_exactly_the_servers_channels() {
    let mut names: Vec<String> = String::from_utf8(register_payload()).unwrap().split('\0').map(str::to_string).collect();
    names.sort();
    let mut want: Vec<String> = SERVER_CHANNELS.iter().map(|p| format!("yet-another-minecraft-bingo:{p}")).collect();
    want.sort();
    assert_eq!(names, want);
}

#[test]
fn it_waits_for_the_player_to_join_then_registers_once_and_sends_the_settings() {
    let mut client = BingoClient::new();
    let mut host = Fake { offline: true, ..Fake::default() };
    for _ in 0..5 {
        client.on_tick(&mut host);
    }
    assert!(host.sent.is_empty(), "nothing is sent before the player joined");
    host.offline = false;
    client.on_tick(&mut host);
    client.on_tick(&mut host);
    client.on_tick(&mut host);
    assert_eq!(registers(&host), 1);
    assert_eq!(host.sent[0].0, REGISTER_CHANNEL, "the channels are registered before anything else is sent");
    assert_eq!(host.sent[0].1, register_payload());
    // The original client sends its settings once the channels are registered.
    let settings: Vec<_> = host.sent.iter().filter(|(c, _)| *c == id("update_settings")).collect();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].1, encode(|w| PlayerSettings::default().write(w)));
    assert_eq!(client.ticks(), 8);
}

#[test]
fn the_state_clock_follows_the_ticks() {
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    for _ in 0..3 {
        client.on_tick(&mut host);
    }
    assert_eq!(client.state.now(), 3 * TICK_MS);
    assert_eq!(TICK_MS, 50);
}

#[test]
fn a_new_join_registers_again_but_the_announce_right_after_registering_does_not() {
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    client.on_tick(&mut host);
    assert_eq!(registers(&host), 1);
    // The server's own announcement arrives a moment after we registered: same join, nothing to redo.
    client.on_packet(REGISTER_CHANNEL, b"yet-another-minecraft-bingo:timer");
    client.on_tick(&mut host);
    assert_eq!(registers(&host), 1);
    // Much later the player joins another server: it announces its channels again.
    for _ in 0..(ANNOUNCE_FOR_TESTS + 5) {
        client.on_tick(&mut host);
    }
    client.on_packet(REGISTER_CHANNEL, b"yet-another-minecraft-bingo:timer");
    client.on_tick(&mut host);
    assert_eq!(registers(&host), 2);
    assert_eq!(host.sent.iter().filter(|(c, _)| *c == id("update_settings")).count(), 2, "the settings go with it");
}

#[test]
fn sounds_are_played_on_the_next_tick_and_no_event_is_left_for_the_host() {
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    client.on_packet(&id("timer"), &5i32.to_be_bytes());
    client.on_packet(&id("card_completed"), &[1]);
    assert!(host.sounds.is_empty(), "a packet only changes state; the sound plays on the tick");
    client.on_tick(&mut host);
    assert_eq!(host.sounds, [(SoundKey::TimerTick, 0.6, 0.875), (SoundKey::GameWon, 1.0, 1.0)]);
    // The winner's fireworks are the client's own (drawn over the card), so the host gets no event for them.
    assert!(client.take_ui_events().is_empty());
}

#[test]
fn ten_thousand_game_messages_leave_no_events_behind() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    let p = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let payload = encode(|w| p.write(w).unwrap());
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    // 10,000 packets between two ticks: the tick takes their events, and the queue must not keep any of them.
    for _ in 0..10_000 {
        client.on_packet(&id("game_message"), &payload);
    }
    client.on_tick(&mut host);
    assert!(client.take_ui_events().len() < 64);
    // And with a tick after each packet, as the host runs them.
    for _ in 0..10_000 {
        client.on_packet(&id("game_message"), &payload);
        client.on_tick(&mut host);
    }
    assert!(client.take_ui_events().len() < 64);
}

#[test]
fn the_stored_settings_go_out_on_registration_instead_of_the_defaults() {
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    client.set_settings(PlayerSettings { bossbar: true, ..PlayerSettings::default() });
    client.on_tick(&mut host);
    client.on_tick(&mut host);
    let settings: Vec<_> = host.sent.iter().filter(|(c, _)| *c == id("update_settings")).collect();
    assert_eq!(settings.len(), 1, "sent once, on registration");
    // The payload is the settings as JSON (prettyPrint): the stored toggle is on, the default one is off.
    let json = String::from_utf8_lossy(&settings[0].1);
    assert!(json.contains("\"bossbar\": true"), "{json}");
    assert!(json.contains("\"scoreboard\": true"), "{json}");
    assert!(!json.contains("\"bossbar\": false"), "{json}");
}

#[test]
fn settings_set_after_the_join_are_sent_on_the_next_tick() {
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    client.on_tick(&mut host);
    assert_eq!(host.sent.iter().filter(|(c, _)| *c == id("update_settings")).count(), 1);
    client.set_settings(PlayerSettings { night_vision: false, ..PlayerSettings::default() });
    client.on_tick(&mut host);
    let settings: Vec<_> = host.sent.iter().filter(|(c, _)| *c == id("update_settings")).collect();
    assert_eq!(settings.len(), 2);
    assert!(String::from_utf8_lossy(&settings[1].1).contains("\"nightVision\": false"));
}

#[test]
fn the_win_fireworks_are_drawn_over_the_hud_card_and_end_when_the_stars_have_faded() {
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    client.on_packet(&id("card_completed"), &[1]);
    client.on_tick(&mut host);
    let mut frame = Fake { image_ok: true, ..Fake::default() };
    client.draw(&mut frame);
    // At the first frame (no time has passed) all 50 stars sit at the card's middle (122 / 2, 132 / 2), with alpha 255.
    let stars: Vec<_> = frame.xf.iter().filter(|g| g.0 == "★").collect();
    assert_eq!(stars.len(), 50);
    for (_, anchor, offset, scale, _, argb, shadow) in &stars {
        assert_eq!(*anchor, (61.0, 66.0));
        assert_eq!(*offset, (-4.0, -4.0));
        assert_eq!(*scale, 1.0);
        assert_eq!(argb >> 24, 255);
        assert!(!shadow);
    }
    // Every star leaves the 100 px range within 80 ticks (speed at least 1.5 - 0.2 = 1.3 px a tick: 104 px), and the
    // firework is dropped with them.
    for _ in 0..80 {
        client.on_tick(&mut host);
    }
    let mut later = Fake { image_ok: true, ..Fake::default() };
    client.draw(&mut later);
    assert!(later.xf.iter().all(|g| g.0 != "★"), "no star is left");
    client.draw(&mut later);
    assert!(later.xf.iter().all(|g| g.0 != "★"));
}

#[test]
fn a_head_whose_skin_never_comes_is_asked_for_five_times_and_then_not_again() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::state::ScoreMessage;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    let mut packet = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 7]))]);
    packet.message = Text(Nbt::Compound(vec![("player".into(), player)]));
    client.state.messages.push(ScoreMessage { created_at: 0, packet });
    for _ in 0..100 {
        client.on_tick(&mut host);
    }
    assert!(client.faces_waiting(), "the skin may still come: the scan keeps coming back");
    for _ in 0..150 {
        client.on_tick(&mut host);
    }
    // The first look is at tick 10, then one try each 60 ticks: tick 10, 70, 130, 190, 250. The fifth is the last:
    // at tick 250 the client gives up, and the scan does not come back for the head (the look at 260 would find none).
    assert_eq!(host.face_calls, vec![7; 5]);
    assert!(!client.faces_waiting(), "given up: the scan stops");
    for _ in 0..750 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls, vec![7; 5]);
    // Another head arriving later makes a look, which must not ask for head 7 a sixth time.
    let mut other = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 8]))]);
    other.message = Text(Nbt::Compound(vec![("player".into(), player)]));
    client.state.messages.push(ScoreMessage { created_at: 0, packet: other });
    for _ in 0..1000 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls.iter().filter(|&&uuid| uuid == 7).count(), 5);
    assert_eq!(host.face_calls.iter().filter(|&&uuid| uuid == 8).count(), 5);
}

#[test]
fn a_head_that_leaves_the_messages_and_comes_back_is_asked_for_again() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::state::ScoreMessage;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    let mut packet = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 7]))]);
    packet.message = Text(Nbt::Compound(vec![("player".into(), player)]));
    let message = ScoreMessage { created_at: 0, packet };
    client.state.messages.push(message.clone());
    for _ in 0..1000 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls.len(), 5, "given up on");
    // The head leaves the messages (a look forgets it), and comes back: its tries start again.
    client.state.messages.clear();
    for _ in 0..10 {
        client.on_tick(&mut host);
    }
    client.state.messages.push(message);
    for _ in 0..10 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls.len(), 6);
}

#[test]
fn a_face_the_store_refuses_is_not_asked_for_again_on_its_own() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::card_images::MAX_FACES;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::state::ScoreMessage;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    // The store keeps MAX_FACES faces: the heads past that are refused, and with them the scan has nothing to wait for.
    let mut host = Fake { image_ok: true, face_ready: true, ..Fake::default() };
    for uuid in 0..(MAX_FACES as i32 + 6) {
        let mut packet = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
        let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, uuid + 1]))]);
        packet.message = Text(Nbt::Compound(vec![("player".into(), player)]));
        client.state.messages.push(ScoreMessage { created_at: 0, packet });
    }
    for _ in 0..1000 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls.len(), MAX_FACES + 6, "each head asked once, and the refused ones not again");
    assert!(!client.faces_waiting(), "a refused face is not waited for");
}

#[test]
fn payloads_that_do_not_decode_are_recorded_and_counted_apart() {
    let mut client = BingoClient::new();
    client.on_packet(&id("timer"), &[0, 0, 7]); // 3 bytes of a 4-byte int
    client.on_packet(&id("timer"), &7i32.to_be_bytes());
    assert_eq!(client.applied[&id("timer")], 1);
    assert_eq!(client.errors.len(), 1);
    assert_eq!(client.errors[0].0, id("timer"));
    for _ in 0..40 {
        client.on_packet(&id("timer"), &[]);
    }
    assert_eq!(client.errors.len(), 32, "only the newest failures are kept");
}

#[test]
fn the_overlay_shows_what_the_real_servers_packets_told_the_client() {
    let cap = |s: &str| {
        let s = s.trim();
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>()
    };
    let mut client = BingoClient::new();
    let mut host = Fake::default();
    client.on_tick(&mut host);
    client.draw(&mut host);
    assert_eq!(host.texts.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(), ["Bingo", "no card"]);

    client.on_packet(&id("display_cards"), &cap(include_str!("captures/display_cards.hex")));
    client.on_packet(&id("update_card_tiles"), &cap(include_str!("captures/update_card_tiles.hex")));
    client.on_packet(&id("timer"), &125i32.to_be_bytes());
    let mut host = Fake::default();
    client.draw(&mut host);
    let lines: Vec<&str> = host.texts.iter().map(|t| t.2.as_str()).collect();
    // 25 tiles of team bingo_red (the capture), and a timer of 125 s = 2:05.
    assert_eq!(lines, ["Bingo", "card of bingo_red: 25 tiles", "timer: 2:05"]);
    // Lines are 10 pixels apart from y = 4, behind one translucent box as wide as the longest line plus padding.
    assert_eq!(host.texts.iter().map(|t| t.1).collect::<Vec<_>>(), [4, 14, 24]);
    let widest = lines.iter().map(|l| 6 * l.chars().count() as i32).max().unwrap();
    assert_eq!(host.rects, [(2, 2, widest + 4, 3 * 10 + 2, 0x78000000)]);
}

// ---- the card, drawn from the original mod's files ----

fn prepared_assets() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bingo-client-assets-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    let png = |name: &str, w: u32, h: u32| {
        let path = dir.join("sprites/bingo").join(format!("{name}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&vec![255u8; (w * h * 4) as usize]).unwrap();
    };
    png("card_red", 122, 132);
    png("card_preview", 122, 132);
    std::fs::write(
        dir.join("card_colors.json"),
        r##"{"teams":{"red":{"textColor":"#ffe3a64b","outlineColor":"#ffff5555","cardTexture":"minecraft:bingo/card_red"}}}"##,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("lang")).unwrap();
    std::fs::write(dir.join("lang/en_us.json"), "{}").unwrap();
    // every registry id up to 1700 is an item named after its number
    let names: String = (0..1700).map(|i| format!("minecraft:item_{i}\n")).collect();
    std::fs::write(dir.join("item_names.txt"), names).unwrap();
    dir
}

fn cap(s: &str) -> Vec<u8> {
    let s = s.trim();
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// The client with two cards on its HUD: the captured card, shown for two teams (red, then blue).
fn client_with_two_cards() -> BingoClient {
    use horizon_bingo::buf::decode;
    use horizon_bingo::display_cards::CardDisplayPacket;
    let mut client = client_with_card();
    let mut p = decode(&cap(include_str!("captures/display_cards.hex")), CardDisplayPacket::read).unwrap();
    let one = p.display[0].1.clone();
    p.display = vec![(Some("bingo_red".into()), one.clone()), (Some("bingo_blue".into()), one)];
    client.on_packet(&id("display_cards"), &encode(|w| p.write(w).unwrap()));
    client
}

fn client_with_card() -> BingoClient {
    let mut client = BingoClient::new();
    client.set_assets(horizon_bingo::assets::Assets::load(&prepared_assets()));
    assert!(client.has_assets(), "{:?}", client.asset_error);
    client.on_packet(&id("display_cards"), &cap(include_str!("captures/display_cards.hex")));
    client.on_packet(&id("update_card_tiles"), &cap(include_str!("captures/update_card_tiles.hex")));
    client
}

#[test]
fn the_atlas_goes_up_once_and_again_only_after_a_refused_upload() {
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    for _ in 0..5 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.uploads.len(), 1, "one upload, not one per tick");
    let mut refusing = Fake { image_ok: false, ..Fake::default() };
    let mut client = client_with_card();
    for _ in 0..3 {
        client.on_tick(&mut refusing);
    }
    assert_eq!(refusing.uploads.len(), 3, "a refused upload is tried again next tick");
}

#[test]
fn the_card_is_drawn_at_the_top_left_with_the_teams_texture_name_and_items() {
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    client.draw(&mut host);
    // The first sprite is the red card's background, the whole 122 x 132 texture, drawn at (0, 0).
    let (src, dst) = host.sprites[0];
    assert_eq!(dst, [0.0, 0.0, 122.0, 132.0]);
    assert_eq!((src[2], src[3]), (122.0, 132.0), "the source is the whole card texture");
    // The team name is the translatable title with its fallback, in the team's text colour, with no shadow.
    // the team name, then the stack counts the card's items carry (white, with a shadow)
    assert_eq!(host.texts[0], (12, 10, "Red Team".to_string(), 0xffe3a64b, false));
    assert_eq!(host.texts[1..].iter().map(|t| (t.2.as_str(), t.3, t.4)).collect::<Vec<_>>(), [("9", 0xffffffff, true), ("10", 0xffffffff, true)]);
    // 25 tiles of the capture: every item icon sits inside the card's tile area at 16 pixels (or smaller for lists).
    assert!(host.items.len() >= 25, "{} items", host.items.len());
    for (name, x, y, size) in &host.items {
        assert!(name.starts_with("minecraft:item_"), "{name}");
        assert!(*x >= 12.0 && *y >= 22.0 && x + size <= 112.0 && y + size <= 122.0 && *size <= 16.0, "({x}, {y}) {size}");
    }
    // the first tile: index 0 sits at (12, 22), its item at (13, 23)
    assert!(host.items.iter().any(|(_, x, y, s)| (*x, *y, *s) == (13.0, 23.0, 16.0)));
}

#[test]
fn without_the_originals_files_it_says_why_instead_of_drawing_nothing() {
    let mut client = BingoClient::new();
    client.set_assets(horizon_bingo::assets::Assets::load(std::path::Path::new("/nonexistent/bingo-assets")));
    assert!(!client.has_assets());
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    assert!(host.uploads.is_empty());
    client.draw(&mut host);
    let lines: Vec<&str> = host.texts.iter().map(|t| t.2.as_str()).collect();
    assert_eq!(lines[0], "Bingo");
    assert!(lines.last().unwrap().starts_with("assets: "), "{lines:?}");
    assert!(host.sprites.is_empty() && host.items.is_empty());
}

#[test]
fn with_no_card_shown_nothing_is_drawn() {
    let mut client = BingoClient::new();
    client.set_assets(horizon_bingo::assets::Assets::load(&prepared_assets()));
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    client.draw(&mut host);
    assert!(host.sprites.is_empty() && host.texts.is_empty() && host.items.is_empty() && host.rects.is_empty());
}

fn oracle_row(file: &str, name: &str) -> Vec<u8> {
    let tsv = std::fs::read_to_string(format!("{}/tests/oracle/{file}.tsv", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let line = tsv.lines().find(|l| l.split('\t').next() == Some(name)).unwrap();
    cap(line.split('\t').nth(2).unwrap())
}

fn with_message(client: &mut BingoClient, message_id: u128) {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    let mut p = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    p.id = message_id;
    p.is_update = false;
    client.on_packet(&id("game_message"), &encode(|w| p.write(w).unwrap()));
}

#[test]
fn score_messages_are_drawn_under_the_card_with_a_shadow_and_expire_after_5_seconds_and_the_fade() {
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    with_message(&mut client, 1);
    // half a second later the message has faded in: its text is drawn below the 132 pixel card, with a shadow
    for _ in 0..12 {
        client.on_tick(&mut host);
    }
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.draw(&mut host);
    let message_lines: Vec<_> = host.texts.iter().filter(|t| t.1 >= 132).collect();
    assert_eq!(message_lines.len(), 1, "{:?}", host.texts);
    assert!(message_lines[0].4, "the message text has a shadow");
    assert_eq!(host.texts.iter().filter(|t| !t.4).count(), 1, "only the team name is drawn without one");
    assert_eq!(client.state.messages.len(), 1);
    // long after 5 seconds and the fade (1 second tick = 20 ticks), the message is gone from the state
    for _ in 0..(20 * 8) {
        client.on_tick(&mut host);
    }
    client.draw(&mut host);
    assert!(client.state.messages.is_empty(), "an expired message is removed");
}

#[test]
fn the_ready_line_needs_the_gui_width_to_be_centred_and_is_skipped_when_it_is_not_known() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::ready_update::ReadyUpdatePacket;
    let mut p = decode(&oracle_row("ready_update", "v2_zero"), ReadyUpdatePacket::read_v2).unwrap();
    p.is_running = true;
    p.title = Some(horizon_bingo::text::Text::plain("Get ready"));
    p.total_millis = 10_000;
    p.remaining_millis = 5_000;
    let payload = encode(|w| p.write_v2(w).unwrap());

    let texts_with = |gui_width: Option<i32>| {
        let mut client = client_with_card();
        client.on_packet(&id("ready_timer_update_v2"), &payload);
        let mut host = Fake { image_ok: true, gui_width, ..Fake::default() };
        client.on_tick(&mut host);
        client.draw(&mut host);
        host.texts.iter().map(|t| (t.0, t.1, t.2.clone())).collect::<Vec<_>>()
    };
    assert!(texts_with(None).iter().all(|t| t.2 != "Get ready"), "no width, no ready line");
    let with = texts_with(Some(427));
    let line = with.iter().find(|t| t.2 == "Get ready").expect("the title");
    // centred: x starts at width / 2 - 50 = 163 (and the text is centred inside the 100 wide bar)
    assert!((163..=263).contains(&line.0), "x = {}", line.0);
    // a different screen width moves it by half the difference (the line is centred)
    let wider = texts_with(Some(627));
    assert_eq!(wider.iter().find(|t| t.2 == "Get ready").unwrap().0 - line.0, 100);
}

// ---- the card screen ----

fn playing_host() -> Fake {
    Fake { image_ok: true, playing: true, gui: Some((427, 240)), ..Fake::default() }
}

fn press_y(client: &mut BingoClient, host: &mut Fake) {
    host.keys = vec!["key.keyboard.y".into()];
    client.on_tick(host);
    host.keys.clear();
    client.on_tick(host);
}

#[test]
fn the_card_key_opens_the_screen_only_while_playing_and_with_something_to_show() {
    let mut client = client_with_card();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    assert_eq!(host.opens, 1);
    assert!(client.screen.is_open());

    // in chat or any other screen the same key is just a letter
    let mut client = client_with_card();
    let mut host = Fake { playing: false, ..playing_host() };
    press_y(&mut client, &mut host);
    assert_eq!(host.opens, 0);
    assert!(!client.screen.is_open());

    // no card and no result: nothing to open
    let mut client = BingoClient::new();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    assert_eq!(host.opens, 0);

    // holding the key does not reopen it every tick, and another key does nothing
    let mut client = client_with_card();
    let mut host = playing_host();
    host.keys = vec!["key.keyboard.y".into()];
    for _ in 0..5 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.opens, 1);
    let mut client = client_with_card();
    let mut host = playing_host();
    host.keys = vec!["key.keyboard.u".into()];
    client.on_tick(&mut host);
    assert_eq!(host.opens, 0);
    // a rebound key
    let mut client = client_with_card();
    client.set_open_key("key.keyboard.k");
    let mut host = playing_host();
    host.keys = vec!["key.keyboard.k".into()];
    client.on_tick(&mut host);
    assert_eq!(host.opens, 1);
}

#[test]
fn the_open_card_packet_opens_the_screen_on_that_team() {
    let mut client = client_with_card();
    let mut host = playing_host();
    let packet = encode(|w| horizon_bingo::open_card::CardOpenPacket { team_key: Some("bingo_red".into()) }.write(w));
    client.on_packet(&id("open_card"), &packet);
    client.on_tick(&mut host);
    assert_eq!(host.opens, 1);
    assert_eq!(client.state.selected_team.as_deref(), Some("bingo_red"));
}

#[test]
fn an_open_screen_draws_its_frame_and_the_card() {
    let mut client = client_with_card();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    let mut frame = playing_host();
    client.render_screen(&mut frame);
    assert!(!frame.sprites.is_empty(), "the screen's sprites");
    assert!(frame.items.len() >= 25, "{} item icons: the card's tiles", frame.items.len());
    // the card (122 x 132) is centred in the main area, so its background is among the sprites
    assert!(frame.sprites.iter().any(|(_, dst)| dst[2] == 122.0 && dst[3] == 132.0), "{:?}", frame.sprites.iter().map(|s| s.1).collect::<Vec<_>>());
    // nothing is drawn without a GUI size
    let mut blind = Fake { gui: None, ..playing_host() };
    client.render_screen(&mut blind);
    assert!(blind.sprites.is_empty() && blind.items.is_empty());
}

#[test]
fn escape_closes_the_screen_with_an_animation_and_then_the_mod_screen() {
    let mut client = client_with_card();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    assert!(client.screen_escape(), "the screen takes Escape");
    assert_eq!(host.closes, 0, "it is still animating out");
    for _ in 0..60 {
        client.on_tick(&mut host);
        let mut frame = playing_host();
        client.render_screen(&mut frame);
    }
    assert_eq!(host.closes, 1, "closed once the animation was done");
    assert!(!client.screen.is_open());
}

#[test]
fn a_screen_the_game_closed_is_forgotten_and_can_be_opened_again() {
    let mut client = client_with_card();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    host.screen_up = false; // the game closed it
    client.on_tick(&mut host);
    assert!(!client.screen.is_open());
    press_y(&mut client, &mut host);
    assert_eq!(host.opens, 2);
}

#[test]
fn the_end_screen_opens_by_itself_and_cannot_be_escaped() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::GameOverPacket;
    let mut client = client_with_card();
    let mut over = decode(&oracle_row("game_over", "read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    over.is_update = false;
    client.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
    let mut host = playing_host();
    client.on_tick(&mut host);
    assert_eq!(host.opens, 1);
    assert!(!client.screen_escape(), "the end screen opens with canEscape = false");
}

#[test]
fn the_hud_card_is_not_drawn_under_the_open_card_screen() {
    let mut client = client_with_card();
    let mut host = playing_host();
    client.on_tick(&mut host);
    let mut hud = playing_host();
    client.draw(&mut hud);
    assert!(hud.items.len() >= 25, "the HUD card is drawn before the screen is open");
    press_y(&mut client, &mut host);
    let mut hud = playing_host();
    client.draw(&mut hud);
    assert!(hud.sprites.is_empty() && hud.items.is_empty() && hud.texts.is_empty(), "nothing of the HUD while the screen is open");
}

#[test]
fn arrow_keys_belong_to_the_open_screen_and_other_keys_do_not() {
    let mut client = client_with_card();
    let mut host = playing_host();
    assert!(!client.screen_key("key.keyboard.left"), "no screen open yet");
    press_y(&mut client, &mut host);
    for _ in 0..3 {
        client.on_tick(&mut host);
        let mut frame = playing_host();
        client.render_screen(&mut frame);
    }
    assert!(client.screen_key("key.keyboard.left") && client.screen_key("key.keyboard.right"));
    assert!(!client.screen_key("key.keyboard.up") && !client.screen_key("key.keyboard.w"));
}

// ---- pictures, scores, history ----

fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 2, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&[255; 16]).unwrap();
    }
    out
}

#[test]
fn a_card_picture_is_packed_into_the_atlas_uploaded_again_and_drawn_instead_of_the_item() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::update_card_tiles::CardTilesPacket;
    let mut client = client_with_card();
    let mut host = playing_host();
    client.on_tick(&mut host);
    let uploads_before = host.uploads.clone();
    assert_eq!(uploads_before.len(), 1);
    // the server sends a picture and a tile that uses it
    client.on_packet(&id("card_image"), &encode(|w| horizon_bingo::card_image::CardImagePacket { id: "pic".into(), image: Some(tiny_png()) }.write(w)));
    let mut tiles = decode(&cap(include_str!("captures/update_card_tiles.hex")), CardTilesPacket::read).unwrap();
    tiles.tiles[0].1.image.texture = Some("pic".into());
    tiles.should_notify = false;
    client.on_packet(&id("update_card_tiles"), &encode(|w| tiles.write(w).unwrap()));
    client.on_tick(&mut host);
    assert_eq!(host.uploads.len(), 2, "the atlas goes up again with the picture in it");
    assert!(host.uploads[1].0 * host.uploads[1].1 > uploads_before[0].0 * uploads_before[0].1 || host.uploads[1] != uploads_before[0]);
    let mut frame = playing_host();
    client.draw(&mut frame);
    assert!(frame.sprites.iter().any(|(_, dst)| *dst == [13.0, 23.0, 16.0, 16.0]), "the picture sits where tile 0's item would");
    assert!(!frame.items.iter().any(|(_, x, y, _)| (*x, *y) == (13.0, 23.0)), "and the item does not");
}

fn open_screen_with(client: &mut BingoClient) -> Fake {
    let mut host = playing_host();
    press_y(client, &mut host);
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    host
}

#[test]
fn score_messages_appear_in_the_history_list_of_the_open_screen() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::text_plain::flatten;
    let mut client = client_with_card();
    let mut msg = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    msg.is_update = false;
    client.on_packet(&id("game_message"), &encode(|w| msg.write(w).unwrap()));
    let expected = flatten(&msg.message, &|_: &str| None);
    let _ = open_screen_with(&mut client);
    let mut frame = playing_host();
    client.render_screen(&mut frame);
    assert!(frame.texts.iter().any(|t| t.2.contains(expected.trim())), "{:?} not in {:?}", expected, frame.texts.iter().map(|t| &t.2).collect::<Vec<_>>());
}

#[test]
fn the_scores_tab_draws_the_teams_of_the_finished_game() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::{EndScreenTab, GameOverPacket};
    let mut client = client_with_card();
    let mut over = decode(&oracle_row("game_over", "v2_winner_team"), GameOverPacket::read_v2).unwrap();
    over.is_update = false;
    over.default_tab = EndScreenTab::Scores;
    assert!(!over.scores.is_empty(), "the oracle row has team rows");
    client.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
    let mut host = playing_host();
    client.on_tick(&mut host);
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    let mut frame = playing_host();
    client.render_screen(&mut frame);
    // no card on the Scores tab: the 122 x 132 card background is not drawn, the scores' text is
    assert!(!frame.sprites.iter().any(|(_, dst)| dst[2] == 122.0 && dst[3] == 132.0), "the cards widget is not drawn on the Scores tab");
    let flat = horizon_bingo::text_plain::flatten(&over.scores[0].name, &|_: &str| None);
    assert!(frame.texts.iter().any(|t| t.2.contains(&flat)), "team name {flat:?} in {:?}", frame.texts.iter().map(|t| &t.2).collect::<Vec<_>>());
    // The score text of each team's row is drawn by the scores tab alone (the history list does not have it).
    for row in &over.scores {
        let want = horizon_bingo::screen_end::score_text(&|_: &str| None, row.score.cards, row.score.lines, row.score.items);
        assert!(frame.texts.iter().any(|t| t.2 == want), "row score {want:?} in {:?}", frame.texts.iter().map(|t| &t.2).collect::<Vec<_>>());
    }
}

#[test]
fn a_card_picture_is_drawn_on_the_open_card_screen_too() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::update_card_tiles::CardTilesPacket;
    let mut client = client_with_card();
    client.on_packet(&id("card_image"), &encode(|w| horizon_bingo::card_image::CardImagePacket { id: "pic".into(), image: Some(tiny_png()) }.write(w)));
    let mut tiles = decode(&cap(include_str!("captures/update_card_tiles.hex")), CardTilesPacket::read).unwrap();
    tiles.tiles[0].1.image.texture = Some("pic".into());
    tiles.should_notify = false;
    client.on_packet(&id("update_card_tiles"), &encode(|w| tiles.write(w).unwrap()));
    let _ = open_screen_with(&mut client);
    let mut frame = playing_host();
    client.render_screen(&mut frame);
    // the picture is 2 x 2 pixels in the atlas: only it has a source rectangle that small
    assert!(frame.sprites.iter().any(|(src, _)| src[2] == 2.0 && src[3] == 2.0), "{:?}", frame.sprites.iter().map(|s| s.0).collect::<Vec<_>>());
}

#[test]
fn a_head_in_a_message_is_asked_for_only_when_it_appears_and_again_only_while_the_skin_is_missing() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::state::ScoreMessage;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    assert!(host.face_calls.is_empty(), "no message, no head to ask for");
    let mut packet = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 7]))]);
    packet.message = Text(Nbt::Compound(vec![("player".into(), player)]));
    client.state.messages.push(ScoreMessage { created_at: 0, packet });
    // the skin is not there yet: it is asked for once, then again after 60 ticks, not on every look
    for _ in 0..50 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls, vec![7]);
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls, vec![7, 7]);
    // once the skin is ready the face goes into the picture, which is sent again, and the head is not asked for again
    host.face_ready = true;
    let uploads = host.uploads.len();
    for _ in 0..200 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls, vec![7, 7, 7]);
    assert_eq!(host.uploads.len(), uploads + 1);
}

#[test]
fn a_head_in_a_hud_message_is_drawn_as_an_8_pixel_sprite_once_its_face_is_in_the_atlas() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_message::GameMessagePacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::state::ScoreMessage;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, face_ready: true, ..Fake::default() };
    client.on_tick(&mut host);
    let mut packet = decode(&oracle_row("game_message", "plain_message"), GameMessagePacket::read).unwrap();
    let player = Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 7]))]);
    packet.message = Text(Nbt::List {
        elem: 10,
        items: vec![Nbt::Compound(vec![("player".into(), player)]), Nbt::Compound(vec![("text".into(), Nbt::String("Bob".into()))])],
    });
    client.state.messages.push(ScoreMessage { created_at: 0, packet });
    for _ in 0..40 {
        client.on_tick(&mut host);
    }
    let mut draw_host = Fake { image_ok: true, ..Fake::default() };
    client.draw(&mut draw_host);
    let heads: Vec<_> = draw_host.sprites.iter().filter(|(_, dst)| dst[2] == 8.0 && dst[3] == 8.0).collect();
    assert_eq!(heads.len(), 1, "{:?}", draw_host.sprites);
    assert_eq!((heads[0].0[2], heads[0].0[3]), (8.0, 8.0));
}

#[test]
fn the_end_screen_of_a_won_game_has_fireworks_and_that_of_a_lost_game_does_not() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::GameOverPacket;
    let stars_after_open = |won: bool| {
        let mut client = client_with_card();
        let mut over = decode(&oracle_row("game_over", "read_v2_minimal"), GameOverPacket::read_v2).unwrap();
        over.is_update = false;
        over.is_winner = won;
        client.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
        let mut host = playing_host();
        client.on_tick(&mut host);
        let mut frame = playing_host();
        client.render_screen(&mut frame);
        frame.xf.iter().filter(|g| g.0 == "★").count()
    };
    // the first frame is at tick 0 of the fireworks: all 50 stars sit at the centre
    assert_eq!(stars_after_open(true), 50);
    assert_eq!(stars_after_open(false), 0);
}

#[test]
fn a_card_opened_from_the_hud_slides_in_from_its_hud_place_and_the_end_screens_card_does_not() {
    let card_at = |client: &mut BingoClient| {
        let mut frame = playing_host();
        client.render_screen(&mut frame);
        frame.sprites.iter().find(|(_, dst)| dst[2] == 122.0 && dst[3] == 132.0).map(|(_, dst)| (dst[0], dst[1]))
    };
    let mut client = client_with_card();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    // the first frame (one tick, 50 ms, after the key): still on its way from the top left corner
    let (x0, y0) = card_at(&mut client).expect("the card is drawn");
    // a second later (the slide takes 150 ms) it has settled in the middle of the screen
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    let (x, y) = card_at(&mut client).expect("the card is drawn");
    assert!(x > 20.0 && y > 20.0, "({x}, {y})");
    assert!(x0 > 0.0 && y0 > 0.0 && x0 < x - 5.0 && y0 < y - 5.0, "({x0}, {y0}) -> ({x}, {y})");
    let again = card_at(&mut client).unwrap();
    assert_eq!(again, (x, y), "settled cards do not move");
    // an end screen has no slide: its card is in the middle at once
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::GameOverPacket;
    let mut end = client_with_card();
    let mut over = decode(&oracle_row("game_over", "read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    over.is_update = false;
    end.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
    let mut host = playing_host();
    end.on_tick(&mut host);
    let (x, y) = card_at(&mut end).expect("the card is drawn");
    assert!(x > 20.0 && y > 20.0, "({x}, {y})");
}

#[test]
fn a_head_in_a_team_name_of_the_game_over_is_asked_for_when_the_game_over_arrives() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::GameOverPacket;
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    let mut client = client_with_card();
    let mut host = Fake { image_ok: true, ..Fake::default() };
    // no game over and no message: nothing to look for
    for _ in 0..20 {
        client.on_tick(&mut host);
    }
    assert!(host.face_calls.is_empty());
    // the game over comes with no message at all; the first team's name is a head (player 9) and its text
    let mut over = decode(&oracle_row("game_over", "v2_winner_team"), GameOverPacket::read_v2).unwrap();
    over.is_update = false;
    let head = Nbt::Compound(vec![(
        "player".into(),
        Nbt::Compound(vec![("id".into(), Nbt::IntArray(vec![0, 0, 0, 9]))]),
    )]);
    let text = Nbt::Compound(vec![("text".into(), Nbt::String("Red".into()))]);
    over.scores[0].name = Text(Nbt::List { elem: 10, items: vec![head, text] });
    client.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
    for _ in 0..10 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.face_calls, vec![9]);
}

impl Fake {
    /// Every draw the host has been asked for so far.
    fn draws(&self) -> usize {
        self.rects.len() + self.texts.len() + self.xf.len() + self.sprites.len() + self.items.len() + self.vanilla.len() + self.tooltips.len()
    }
}

#[test]
fn the_card_screen_fades_its_cards_but_not_the_one_sliding_in_from_the_hud() {
    // The key's first tick opens the screen at 50 ms and the second one (the render) is at 100 ms: 50 ms into the
    // 150 ms EXP_OUT, which is 1 - 2^(-10/3) = 0.900787 (0.9008 to four places).
    let mut client = client_with_two_cards();
    let mut host = playing_host();
    press_y(&mut client, &mut host);
    let mut frame = playing_host();
    client.render_screen(&mut frame);
    assert!(!frame.fade_begins.is_empty(), "the cards fade as the screen opens");
    assert_eq!(frame.fade_begins.len(), frame.fade_ends.len());
    for (_, alpha) in &frame.fade_ends {
        assert!((alpha - 0.900_787).abs() < 1e-4, "{alpha}");
    }
    // The selected card is the one that slides in: it is the only card picture drawn at full alpha, and the others fade.
    let unfaded = frame.card_faded.iter().filter(|f| !**f).count();
    assert_eq!(unfaded, 1, "{:?}", frame.card_faded);
    assert!(frame.card_faded.len() >= 2, "{:?}", frame.card_faded);

    // After the 150 ms the slide is over and no card is special: the alpha no longer changes a byte, so nothing fades.
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    let mut done = playing_host();
    client.render_screen(&mut done);
    assert!(done.fade_begins.is_empty() && done.fade_ends.is_empty(), "{:?}", done.fade_ends);
    assert_eq!(done.card_faded.len(), frame.card_faded.len());
    assert!(done.card_faded.iter().all(|f| !f), "{:?}", done.card_faded);
}

#[test]
fn the_end_screens_cards_have_no_entrance_so_every_one_fades() {
    use horizon_bingo::buf::decode;
    use horizon_bingo::game_over::GameOverPacket;
    let mut end = client_with_two_cards();
    let mut over = decode(&oracle_row("game_over", "read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    over.is_update = false;
    end.on_packet(&id("game_over_v2"), &encode(|w| over.write_v2(w).unwrap()));
    let mut host = playing_host();
    end.on_tick(&mut host);
    let mut frame = playing_host();
    end.render_screen(&mut frame);
    assert_eq!(frame.fade_begins.len(), 1, "one fade over the whole row");
    assert_eq!(frame.fade_ends.len(), 1);
    assert!(frame.card_faded.len() >= 2 && frame.card_faded.iter().all(|f| *f), "{:?}", frame.card_faded);
}

// ---- the HUD's keys and render gate (`hud_keys`) ----

/// The `ready_timer_update_v2` payload of a ready state: the oracle's zero packet with these flags, titled "Get ready".
fn ready_payload(can_send_ready: bool, is_running: bool, is_ready: bool) -> Vec<u8> {
    use horizon_bingo::buf::decode;
    use horizon_bingo::ready_update::ReadyUpdatePacket;
    let mut p = decode(&oracle_row("ready_update", "v2_zero"), ReadyUpdatePacket::read_v2).unwrap();
    p.can_send_ready = can_send_ready;
    p.is_running = is_running;
    p.is_ready = is_ready;
    p.title = Some(horizon_bingo::text::Text::plain("Get ready"));
    p.total_millis = 10_000;
    p.remaining_millis = 5_000;
    encode(|w| p.write_v2(w).unwrap())
}

/// A key held for one tick and let go: the handler sees one press.
fn tap(client: &mut BingoClient, host: &mut Fake, key: &str) {
    host.keys = vec![key.into()];
    client.on_tick(host);
    host.keys.clear();
    client.on_tick(host);
}

/// The payloads sent on `set_ready_v2`, in order.
fn ready_sends(host: &Fake) -> Vec<Vec<u8>> {
    host.sent.iter().filter(|(c, _)| c == &id("set_ready_v2")).map(|(_, data)| data.clone()).collect()
}

/// One frame of the HUD: how many card sprites it drew, and whether it drew the ready line.
fn hud_frame(client: &mut BingoClient, host: &mut Fake) -> (usize, bool) {
    host.sprites.clear();
    host.texts.clear();
    client.draw(host);
    (host.sprites.len(), host.texts.iter().any(|t| t.2 == "Get ready"))
}

#[test]
fn sneak_with_the_card_key_readies_up_only_when_the_player_may_and_the_timer_runs() {
    let ready_up = |can_send: bool, running: bool, ready: bool| {
        let mut client = client_with_card();
        client.on_packet(&id("ready_timer_update_v2"), &ready_payload(can_send, running, ready));
        let mut host = playing_host();
        host.sneak = true;
        tap(&mut client, &mut host, "key.keyboard.y");
        (ready_sends(&host), host.opens)
    };
    // Not ready yet: `SetReadyPacket(true)` as V2, the flag and then "no option" (a 0 byte).
    assert_eq!(ready_up(true, true, false), (vec![vec![1, 0]], 0));
    // Already ready: the same keys take the ready state back.
    assert_eq!(ready_up(true, true, true), (vec![vec![0, 0]], 0));
    // The player may not ready up: the card opens instead and nothing is sent.
    assert_eq!(ready_up(false, true, false), (vec![], 1));
    // The timer is stopped: neither a packet nor the card.
    assert_eq!(ready_up(true, false, false), (vec![], 0));

    // Holding the keys sends the ready packet once.
    let mut client = client_with_card();
    client.on_packet(&id("ready_timer_update_v2"), &ready_payload(true, true, false));
    let mut host = playing_host();
    host.sneak = true;
    host.keys = vec!["key.keyboard.y".into()];
    for _ in 0..5 {
        client.on_tick(&mut host);
    }
    assert_eq!(ready_sends(&host).len(), 1);
}

#[test]
fn the_hud_toggle_key_flips_the_config_and_tells_the_host_it_once() {
    use horizon_bingo::config::ClientConfig;
    let mut client = client_with_card();
    client.set_toggle_key("key.keyboard.h");
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    assert!(hud_frame(&mut client, &mut host).0 > 0, "the card is shown by default");
    assert!(host.configs.is_empty(), "nothing is saved until the player changes it");

    tap(&mut client, &mut host, "key.keyboard.h");
    assert_eq!(hud_frame(&mut client, &mut host), (0, false), "the card is gone");
    assert!(!client.config().enable_hud);
    assert_eq!(host.configs.len(), 1, "the host is told once, on the tick after the press");
    assert!(host.configs[0].contains("\"enableHud\": false"), "{}", host.configs[0]);

    tap(&mut client, &mut host, "key.keyboard.h");
    assert!(hud_frame(&mut client, &mut host).0 > 0, "and back again");
    assert_eq!(host.configs.len(), 2);
    assert!(ClientConfig::from_json(&host.configs[1]).enable_hud);
}

#[test]
fn a_config_loaded_with_the_hud_off_hides_the_card_and_is_not_written_back() {
    use horizon_bingo::config::ClientConfig;
    let mut client = client_with_card();
    client.set_config(ClientConfig { enable_hud: false, ..ClientConfig::default() });
    let mut host = Fake { image_ok: true, ..Fake::default() };
    client.on_tick(&mut host);
    assert_eq!(hud_frame(&mut client, &mut host), (0, false));
    assert!(host.configs.is_empty(), "the config file is the host's own copy: the mod does not send it back");

    client.set_config(ClientConfig::default());
    client.on_tick(&mut host);
    assert!(hud_frame(&mut client, &mut host).0 > 0, "a reloaded config with the HUD on shows the card");
    assert!(host.configs.is_empty());
}

#[test]
fn the_settings_screen_hides_the_card_and_the_toggle_key_shows_it_again() {
    use horizon_bingo::config::ClientConfig;
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    client.render_screen(&mut host);
    // The enable HUD row sits under the quick start row: its widget is 150 by 20 at (213, 86).
    client.screen_press((427, 240), (250, 96), &width6);
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
    assert!(!ClientConfig::from_json(&host.configs[0]).enable_hud);

    assert!(client.screen_escape());
    client.on_tick(&mut host);
    assert_eq!(client.screen_kind(), None);
    assert_eq!(hud_frame(&mut client, &mut host), (0, false), "the settings hid the card");

    client.set_toggle_key("key.keyboard.h");
    tap(&mut client, &mut host, "key.keyboard.h");
    assert!(hud_frame(&mut client, &mut host).0 > 0, "the toggle key shows it again");
    assert_eq!(host.configs.len(), 2);
    assert!(ClientConfig::from_json(&host.configs[1]).enable_hud);
}

#[test]
fn f3_tab_f1_and_the_hud_toggle_each_hide_only_what_they_should() {
    let mut client = client_with_card();
    client.on_packet(&id("ready_timer_update_v2"), &ready_payload(false, true, false));
    let mut host = Fake { image_ok: true, gui_width: Some(427), ..Fake::default() };
    client.on_tick(&mut host);
    let (cards, ready) = hud_frame(&mut client, &mut host);
    assert!(cards > 0 && ready, "both drawn by default");

    host.f3 = true;
    assert_eq!(hud_frame(&mut client, &mut host), (0, false), "F3 hides the card and the ready line");
    host.f3 = false;
    assert!(hud_frame(&mut client, &mut host).0 > 0, "the card is back when F3 is closed");

    host.tab = true;
    let (cards, ready) = hud_frame(&mut client, &mut host);
    assert!(cards > 0 && !ready, "Tab hides only the ready line");
    host.tab = false;

    host.f1_hidden = true;
    assert_eq!(hud_frame(&mut client, &mut host), (0, false), "the F1 hide hides everything");
    host.f1_hidden = false;

    // The HUD toggle hides only the card: the ready line stays, as `enableHud` only switches the cards.
    client.set_toggle_key("key.keyboard.h");
    tap(&mut client, &mut host, "key.keyboard.h");
    assert!(!client.config().enable_hud, "the toggle flips the config's HUD flag");
    assert_eq!(hud_frame(&mut client, &mut host), (0, true), "the card is off, the ready line is on");
}

// ---- the client config: the card's place, the messages' time, the sounds' volume ----

#[test]
fn the_config_puts_the_card_at_the_bottom_right_of_the_window_at_its_scale() {
    use horizon_bingo::config::{CardAlignment, ClientConfig};
    let mut client = client_with_card();
    client.set_config(ClientConfig { card_alignment: CardAlignment::BottomRight, ..ClientConfig::default() });
    let mut host = Fake { image_ok: true, gui: Some((427, 240)), ..Fake::default() };
    client.on_tick(&mut host);
    client.draw(&mut host);
    // The window's corner less the card: 427 - 122 and 240 - 132.
    assert_eq!(host.sprites[0].1, [305.0, 108.0, 122.0, 132.0]);
    // The team name keeps its place inside the card, 12 and 10 pixels from its corner.
    assert_eq!((host.texts[0].0, host.texts[0].1), (317, 118));

    // At scale 2 the card is 244 by 264: 427 - 244 and 240 - 264, so its top runs above the window.
    client.set_config(ClientConfig { card_alignment: CardAlignment::BottomRight, card_scale: 2.0, ..ClientConfig::default() });
    let mut host = Fake { image_ok: true, gui: Some((427, 240)), ..Fake::default() };
    client.on_tick(&mut host);
    client.draw(&mut host);
    assert_eq!(host.sprites[0].1, [183.0, -24.0, 244.0, 264.0]);
}

#[test]
fn a_top_right_card_is_moved_in_from_the_right_by_its_offset_and_down_by_the_other() {
    use horizon_bingo::config::{CardAlignment, ClientConfig};
    let mut client = client_with_card();
    client.set_config(ClientConfig { card_alignment: CardAlignment::TopRight, card_offset_x: 5, card_offset_y: 7, ..ClientConfig::default() });
    let mut host = Fake { image_ok: true, gui: Some((427, 240)), ..Fake::default() };
    client.on_tick(&mut host);
    client.draw(&mut host);
    // x: 427 - 122 is the right edge, the offset moves it in by 5; y: the top, moved down by 7.
    assert_eq!(host.sprites[0].1, [300.0, 7.0, 122.0, 132.0]);
}

#[test]
fn the_message_duration_in_the_config_decides_how_long_a_message_stays() {
    use horizon_bingo::config::ClientConfig;
    let remaining = |seconds: i64| {
        let mut client = client_with_card();
        client.set_config(ClientConfig { message_duration_seconds: seconds, ..ClientConfig::default() });
        let mut host = Fake { image_ok: true, ..Fake::default() };
        client.on_tick(&mut host);
        with_message(&mut client, 1);
        // 3 seconds after the message: a 1 second message (and its 250 ms fade) is gone, the default 5 seconds keeps it.
        for _ in 0..60 {
            client.on_tick(&mut host);
        }
        client.draw(&mut host);
        client.state.messages.len()
    };
    assert_eq!(remaining(1), 0);
    assert_eq!(remaining(5), 1);
}

#[test]
fn a_configured_sound_volume_replaces_the_default_for_that_sound_only() {
    use horizon_bingo::config::ClientConfig;
    use std::collections::BTreeMap;
    let mut client = BingoClient::new();
    let volumes: BTreeMap<String, f32> = [("game_won".to_string(), 0.4f32)].into_iter().collect();
    client.set_config(ClientConfig { sound_volumes: volumes, ..ClientConfig::default() });
    let mut host = Fake::default();
    client.on_packet(&id("card_completed"), &[1]);
    client.on_tick(&mut host);
    // The host scales the call's volume by the default 0.8, so the host gets the call's volume times 0.4 / 0.8.
    let (_, volume, pitch) = *host.sounds.iter().find(|s| s.0 == SoundKey::GameWon).expect("the game won sound");
    assert!((volume - 0.5).abs() < 1e-6, "{volume}");
    assert_eq!(pitch, 1.0);
}

// ---- the settings screen ----

fn width6(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

#[test]
fn the_settings_key_opens_the_settings_screen_once_per_press_and_escape_closes_it() {
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    // Held for three ticks: one press, one screen.
    host.keys = vec!["key.keyboard.o".into()];
    for _ in 0..3 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.opens, 1);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Settings));

    // It covers the world with the menu background (0, 0 to 427 x 240, 25% black), and its title is drawn.
    client.render_screen(&mut host);
    assert_eq!(host.rects.first(), Some(&(0, 0, 427, 240, 0x4000_0000)));
    assert!(host.texts.iter().any(|t| t.2 == "bingo.full_name"), "{:?}", host.texts);

    // Escape is the screen's; the screen closes on the next tick and so does the host's.
    assert!(client.screen_escape());
    client.on_tick(&mut host);
    assert_eq!(client.screen_kind(), None);
    assert_eq!(host.closes, 1);
    assert!(!host.screen_is_open());

    // The key is still held, and it was pressed once: it does not open the screen again.
    for _ in 0..2 {
        client.on_tick(&mut host);
    }
    assert_eq!(host.opens, 1);

    // Let go, then press again: a new press opens it.
    host.keys.clear();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    assert_eq!(host.opens, 2);
}

#[test]
fn a_click_on_a_setting_changes_the_config_and_the_host_is_told_the_new_config_on_the_next_tick() {
    use horizon_bingo::config::ClientConfig;
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    client.render_screen(&mut host);
    // The first setting is the quick start button. Its widget is 150 by 20 at (213, 46): the list's right edge less
    // 4 px, and its row's line at y 44 plus 2.
    client.screen_press((427, 240), (250, 56), &width6);
    assert!(host.configs.is_empty(), "the host is told on the next tick, not within the click");
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
    let saved = ClientConfig::from_json(&host.configs[0]);
    assert_eq!(saved, ClientConfig { show_quick_start_button: false, ..ClientConfig::default() });
    assert_eq!(client.config(), &saved);
    // A tick with no change tells the host nothing.
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
}

#[test]
fn the_test_button_of_a_sound_plays_it_at_its_configured_volume_and_changes_nothing() {
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    client.render_screen(&mut host);
    // 17 notches of 24 px scroll the list 408 px: the item_scored test button, 532 px down the list, is at 124 px.
    client.screen_wheel((250, 100), -17.0);
    client.render_screen(&mut host);
    client.screen_press((427, 240), (250, 136), &width6);
    client.on_tick(&mut host);
    assert_eq!(host.test_sounds, [("item_scored".to_string(), 0.8)]);
    assert!(host.configs.is_empty());
}

// ---- the player tooltip ----

#[test]
fn a_player_looked_at_for_20_ticks_shows_a_tooltip_that_fades_in_and_out() {
    use horizon_bingo::text::Text;
    use horizon_bingo::tooltip_v2::TooltipPacket;
    let mut client = client_with_card();
    let uuid: u128 = 0x1234;
    client.state.tooltips_by_entity.insert(uuid, TooltipPacket { text: vec![Text::plain("Steve")], entity_id: uuid });
    let mut host = playing_host();
    host.look = Some(uuid);
    // The look starts on the first tick; 20 ticks later (tick 21) the tooltip shows.
    client.on_tick(&mut host);
    for _ in 0..19 {
        client.on_tick(&mut host);
    }
    client.draw(&mut host);
    assert!(host.tooltips.is_empty(), "19 ticks of looking are not enough");
    client.on_tick(&mut host);
    client.draw(&mut host);
    assert_eq!(host.tooltips, [vec!["Steve".to_string()]]);
    // It starts from alpha 0, the fade-in's start, and it is drawn between begin_alpha and end_alpha.
    assert_eq!(host.fade_ends.last().map(|e| e.1), Some(0.0));
    // A quarter second on (tick 26, 1300 ms) the fade-in is done.
    for _ in 0..5 {
        client.on_tick(&mut host);
    }
    host.tooltips.clear();
    client.draw(&mut host);
    assert_eq!(host.tooltips, [vec!["Steve".to_string()]]);
    assert_eq!(host.fade_ends.last().map(|e| e.1), Some(1.0));
    // Looking away: the fade-out starts 100 ms after the last look (1300 ms). At tick 29 (1450 ms) it is at 0.8, which
    // is 1 - (1450 - 1400) / 250.
    host.look = None;
    for _ in 0..3 {
        client.on_tick(&mut host);
    }
    host.tooltips.clear();
    client.draw(&mut host);
    assert_eq!(host.tooltips, [vec!["Steve".to_string()]]);
    assert!((host.fade_ends.last().unwrap().1 - 0.8).abs() < 1e-6, "{:?}", host.fade_ends.last());
    // The fade-out ends 350 ms after the last look (1650 ms, tick 33): nothing is drawn from then on.
    for _ in 0..4 {
        client.on_tick(&mut host);
    }
    host.tooltips.clear();
    client.draw(&mut host);
    assert!(host.tooltips.is_empty());
}

#[test]
fn nothing_shows_when_the_crosshair_is_on_nothing_or_on_a_player_without_a_tooltip() {
    let mut client = client_with_card();
    let mut host = playing_host();
    host.look = Some(7);
    for _ in 0..30 {
        client.on_tick(&mut host);
    }
    client.draw(&mut host);
    assert!(host.tooltips.is_empty());
}


#[test]
fn a_reload_of_the_saved_file_does_not_undo_a_change_made_after_the_save() {
    use horizon_bingo::config::ClientConfig;
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    client.render_screen(&mut host);
    // Enable HUD off, saved on the next tick.
    client.screen_press((427, 240), (250, 96), &width6);
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
    // Show Multiple Cards on, before the next tick. The file's reload (the text saved before this change) arrives first.
    client.render_screen(&mut host);
    client.screen_press((427, 240), (250, 168), &width6);
    client.set_config(ClientConfig::from_json(&host.configs[0]));
    client.on_tick(&mut host);
    let saved = ClientConfig::from_json(host.configs.last().expect("saved on the tick"));
    assert!(!saved.enable_hud, "the first change stays");
    assert!(saved.show_multiple_cards, "the second change is saved, not undone by the stale reload");
    assert!(client.config().show_multiple_cards);
}

// ---- the card placement screen, from the settings screen's card position row ----

/// The settings screen is open and its card position row was clicked. The row's button is 150 x 20 at (213, 110): the
/// list's right edge less 4 px, and the row (at y 108) plus 2. The placement screen's Cancel is at (59, 212) and its Ok
/// at (217, 212), both 150 x 20, so their centres are (134, 222) and (292, 222) below. Its card starts at (0, 0), 122 x 132.
fn placement_up() -> (BingoClient, Fake) {
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    assert_eq!(client.screen_kind(), Some(ScreenKind::Settings));
    client.render_screen(&mut host);
    client.screen_press((427, 240), (250, 120), &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    (client, host)
}

const OK_AT: (i32, i32) = (292, 222);
const CANCEL_AT: (i32, i32) = (134, 222);

#[test]
fn the_card_position_row_puts_the_placement_screen_on_the_same_mod_screen() {
    let (mut client, mut host) = placement_up();
    // One screen was opened for the settings and it is still the one up: nothing closed or opened again.
    assert_eq!(host.opens, 1);
    assert_eq!(host.closes, 0);
    assert!(host.screen_is_open());
    client.on_tick(&mut host);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    assert!(host.configs.is_empty(), "opening the placement screen changes no config");
}

#[test]
fn the_card_position_click_opens_the_placement_screen_and_the_release_and_later_frames_keep_it() {
    // The live order: the settings screen is drawn, the press on its card position button opens the placement screen,
    // then the release, the ticks and the frames come, and the placement screen is still the one up.
    let mut client = client_with_card();
    client.set_settings_key("key.keyboard.o");
    let mut host = playing_host();
    client.on_tick(&mut host);
    tap(&mut client, &mut host, "key.keyboard.o");
    client.render_screen(&mut host);
    client.screen_press((427, 240), (287, 119), &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    assert!(client.screen_release());
    for _ in 0..3 {
        client.on_tick(&mut host);
        client.render_screen(&mut host);
    }
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    assert_eq!(host.closes, 0);
    assert!(host.configs.is_empty(), "opening the placement screen changes no config");
}

#[test]
fn a_drag_moves_the_card_and_ok_writes_its_offsets_then_returns_to_the_settings_screen() {
    use horizon_bingo::config::ClientConfig;
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    // A drag from the body (50, 50) to (80, 90) moves the card by (30, 40), from (0, 0) to (30, 40).
    assert!(client.screen_drag((50, 50)));
    assert!(client.screen_drag((80, 90)));
    client.render_screen(&mut host);
    // While it is moved, the card is under the drag overlay (0x80ffffff) at its new place.
    assert!(host.rects.contains(&(30, 40, 122, 132, 0x80ff_ffff)), "{:?}", host.rects);
    assert!(client.screen_release());
    // Nothing is written until Ok.
    client.on_tick(&mut host);
    assert!(host.configs.is_empty());
    assert_eq!(client.config(), &ClientConfig::default());

    client.render_screen(&mut host);
    client.screen_press((427, 240), OK_AT, &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Settings), "Ok returns to the settings screen");
    assert!(host.screen_is_open(), "the mod's screen stays up");
    assert_eq!(host.closes, 0);
    let expected = ClientConfig { card_offset_x: 30, card_offset_y: 40, ..ClientConfig::default() };
    assert_eq!(client.config(), &expected);
    // The host is told once, on the next tick.
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
    assert_eq!(ClientConfig::from_json(&host.configs[0]), expected);
    client.on_tick(&mut host);
    assert_eq!(host.configs.len(), 1);
}

#[test]
fn a_card_dragged_to_the_bottom_right_is_saved_from_its_nearest_corner() {
    use horizon_bingo::config::{CardAlignment, ClientConfig};
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    // The drag moves the card by (300, 150): its right edge is at 422, 5 short of the window's 427, and its bottom at
    // 282, 42 past the window's 240. Released, the card is pulled back to the bottom: its top left is at (300, 108).
    assert!(client.screen_drag((50, 50)));
    assert!(client.screen_drag((350, 200)));
    assert!(client.screen_release());
    client.render_screen(&mut host);
    client.screen_press((427, 240), OK_AT, &width6);
    // Saved from the right and the bottom: 5 px from the right edge, 0 from the bottom.
    let expected = ClientConfig { card_alignment: CardAlignment::BottomRight, card_offset_x: 5, card_offset_y: 0, ..ClientConfig::default() };
    assert_eq!(client.config(), &expected);
    client.on_tick(&mut host);
    assert_eq!(ClientConfig::from_json(&host.configs[0]), expected);
}

#[test]
fn a_drag_from_the_right_edge_resizes_the_card_and_ok_saves_the_scale() {
    use horizon_bingo::config::ClientConfig;
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    // (122, 0) is on the right handle, so the drag resizes: 183 px from the card's corner against 122 at the start is 1.5.
    assert!(client.screen_drag((122, 0)));
    assert!(client.screen_drag((183, 0)));
    assert!(client.screen_release());
    client.render_screen(&mut host);
    client.screen_press((427, 240), OK_AT, &width6);
    let expected = ClientConfig { card_scale: 1.5, ..ClientConfig::default() };
    assert_eq!(client.config(), &expected);
    client.on_tick(&mut host);
    assert_eq!(ClientConfig::from_json(&host.configs[0]), expected);
}

#[test]
fn escape_cancels_the_placement_and_returns_to_the_settings_screen_with_the_config_as_it_was() {
    use horizon_bingo::config::ClientConfig;
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    assert!(client.screen_drag((50, 50)));
    assert!(client.screen_drag((80, 90)));
    assert!(client.screen_release());
    // Escape is the placement screen's: it takes the key, and the settings screen is back.
    assert!(client.screen_escape());
    assert_eq!(client.screen_kind(), Some(ScreenKind::Settings));
    client.on_tick(&mut host);
    assert!(host.configs.is_empty(), "a cancelled placement writes nothing");
    assert_eq!(client.config(), &ClientConfig::default());
    assert!(host.screen_is_open());
    assert_eq!(host.closes, 0);
    // The settings screen is still up, so its Escape closes it as before.
    assert!(client.screen_escape());
    client.on_tick(&mut host);
    assert_eq!(client.screen_kind(), None);
    assert_eq!(host.closes, 1);
}

#[test]
fn the_cancel_button_returns_to_the_settings_screen_and_writes_nothing() {
    use horizon_bingo::config::ClientConfig;
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    assert!(client.screen_drag((50, 50)));
    assert!(client.screen_drag((90, 90)));
    assert!(client.screen_release());
    client.render_screen(&mut host);
    client.screen_press((427, 240), CANCEL_AT, &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Settings));
    client.on_tick(&mut host);
    assert!(host.configs.is_empty());
    assert_eq!(client.config(), &ClientConfig::default());
}

#[test]
fn a_click_on_the_card_itself_does_nothing_and_the_keys_and_wheel_stay_with_the_placement_screen() {
    use horizon_bingo::config::ClientConfig;
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    client.screen_press((427, 240), (50, 50), &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    client.on_tick(&mut host);
    assert!(host.configs.is_empty());
    assert_eq!(client.config(), &ClientConfig::default());
    // The arrow keys are the game's, as on the settings screen; the wheel is taken and scrolls nothing.
    assert!(!client.screen_key("key.keyboard.left"));
    assert!(client.screen_wheel((100, 100), -1.0));
}

#[test]
fn reopening_the_placement_starts_from_the_saved_card() {
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    assert!(client.screen_drag((50, 50)));
    assert!(client.screen_drag((80, 90)));
    assert!(client.screen_release());
    client.render_screen(&mut host);
    client.screen_press((427, 240), OK_AT, &width6);
    // Back on the settings screen: its card position row opens the placement screen again, on the card at (30, 40).
    client.render_screen(&mut host);
    client.screen_press((427, 240), (250, 120), &width6);
    assert_eq!(client.screen_kind(), Some(ScreenKind::Placement));
    client.render_screen(&mut host);
    host.rects.clear();
    // A drag from (40, 50), inside the card at (30, 40), moves it by (10, 10): the overlay is at (40, 50).
    assert!(client.screen_drag((40, 50)));
    assert!(client.screen_drag((50, 60)));
    client.render_screen(&mut host);
    assert!(host.rects.contains(&(40, 50, 122, 132, 0x80ff_ffff)), "{:?}", host.rects);
}

#[test]
fn the_placement_screen_draws_its_buttons_and_help_text_with_a_shadow() {
    let (mut client, mut host) = placement_up();
    client.render_screen(&mut host);
    // Cancel's label is centred on its button: 59 + 75 - 36 / 2 = 116, and 212 + (20 - 8) / 2 = 218 down.
    assert!(host.texts.contains(&(116, 218, "Cancel".to_string(), 0xffff_ffff, true)), "{:?}", host.texts);
    // The help's first line is centred at the top: 213 less half its width, at y 8, in green.
    let help = "Left click and drag the center of the card to move it around the screen.";
    let help_x = 213 - 6 * help.chars().count() as i32 / 2;
    assert!(host.texts.contains(&(help_x, 8, help.to_string(), 0xffa3_f5a3, true)), "{:?}", host.texts);
}
