//! GameMessagePacket against the real mod. Every row of tests/oracle.tsv came from OracleGameMessage.java, which runs
//! the mod's own GameMessagePacket.V1 converter (and CardTileImage.V1 inside it) over PacketBufImpl.
//!
//! The expected values in `case` are written by hand from the Kotlin source. Item and text shapes follow the rows the
//! W2 oracle already verified (tests/oracle/w2.tsv in exp-skeleton), so they are not derived from this crate's code.

use horizon_bingo::game_message::{GameMessagePacket, MessageType, ID};
use horizon_bingo::buf::{decode, Writer};
use horizon_bingo::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use horizon_bingo::game_status::JavaDuration;
use horizon_bingo::item::{component, ComponentValue, ItemStack};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;

const TSV: &str = include_str!("oracle/game_message.tsv");

/// Every case the oracle writes, in its order. One row each.
const CASES: [&str; 16] = [
    "minimal",
    "styled_message",
    "plain_message",
    "non_ascii",
    "type_leading_team",
    "type_line_scored",
    "type_item_scored",
    "type_card_completed",
    "uuid_high_bit",
    "duration_max_millis",
    "duration_min_millis",
    "duration_overflow",
    "image_item_named_lore",
    "image_no_item",
    "list_three",
    "nullables_empty_strings",
];

/// `new UUID(0x0102030405060708L, 0x090A0B0C0D0E0F10L)`.
const ORDINARY_ID: u128 = 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10;

struct Row {
    name: String,
    channel: String,
    hex: String,
}

fn rows() -> Vec<Row> {
    TSV.lines()
        .map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 3, "bad row: {line}");
            Row { name: cols[0].to_string(), channel: cols[1].to_string(), hex: cols[2].to_string() }
        })
        .collect()
}

fn bytes_of(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex")).collect()
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn encode(packet: &GameMessagePacket) -> Vec<u8> {
    let mut w = Writer::default();
    packet.write(&mut w).expect("the packet writes");
    w.into_bytes()
}

/// A diamond sword (registry id 1050, from the W2 rows) with custom_name "Excalibur" and lore "line1" plus an italic
/// "line2". Components are in the order the oracle sets them.
fn named_sword() -> ItemStack {
    ItemStack {
        count: 1,
        item: 1050,
        added: vec![
            (component::CUSTOM_NAME, ComponentValue::Text(Text::plain("Excalibur"))),
            (
                component::LORE,
                ComponentValue::TextList(vec![
                    Text::plain("line1"),
                    Text(Nbt::compound(vec![("text", Nbt::string("line2")), ("italic", Nbt::Byte(1))])),
                ]),
            ),
        ],
        removed: vec![],
    }
}

/// Stone (registry id 1) with count 3 and one lore line "L".
fn stone_three_with_lore() -> ItemStack {
    ItemStack {
        count: 3,
        item: 1,
        added: vec![(component::LORE, ComponentValue::TextList(vec![Text::plain("L")]))],
        removed: vec![],
    }
}

fn image(
    item: Option<ItemStack>,
    texture: Option<&str>,
    item_style: ItemStyle,
    decoration: Decoration,
    decoration_texture: Option<&str>,
    is_achieved: bool,
) -> CardTileImage {
    CardTileImage {
        item,
        texture: texture.map(str::to_string),
        item_style,
        decoration,
        decoration_texture: decoration_texture.map(str::to_string),
        is_achieved,
    }
}

fn plain_image() -> CardTileImage {
    image(None, None, ItemStyle::Normal, Decoration::None, None, false)
}

fn named_image() -> CardTileImage {
    image(Some(named_sword()), None, ItemStyle::OneOf, Decoration::Advancement, Some("adv"), true)
}

fn no_item_image() -> CardTileImage {
    image(None, Some("tex/none.png"), ItemStyle::AllOf, Decoration::MultiItem, None, false)
}

fn packet(
    id: u128,
    time: JavaDuration,
    team: &str,
    image: CardTileImage,
    image_list: Vec<CardTileImage>,
    message_type: MessageType,
    message: Text,
    is_update: bool,
) -> GameMessagePacket {
    GameMessagePacket {
        id,
        time_elapsed: time,
        team: team.to_string(),
        image,
        image_list,
        message_type,
        message,
        is_update,
    }
}

fn zero_duration() -> JavaDuration {
    JavaDuration { seconds: 0, nanos: 0 }
}

/// The packet the oracle wrote for `name`.
fn case(name: &str) -> GameMessagePacket {
    match name {
        // Duration.ZERO, no image, no list, empty text, empty team.
        "minimal" => packet(0, zero_duration(), "", plain_image(), vec![], MessageType::LeadingTeam, Text::empty(), false),
        // Duration.ofMillis(41893), red and bold text (keys color, text, bold).
        "styled_message" => packet(
            ORDINARY_ID,
            JavaDuration { seconds: 41, nanos: 893_000_000 },
            "bingo_red",
            plain_image(),
            vec![],
            MessageType::LineScored,
            Text(Nbt::compound(vec![
                ("color", Nbt::string("red")),
                ("text", Nbt::string("Team Red")),
                ("bold", Nbt::Byte(1)),
            ])),
            false,
        ),
        // Duration.ofSeconds(90), plain text, isUpdate true.
        "plain_message" => packet(
            1,
            JavaDuration { seconds: 90, nanos: 0 },
            "bingo_blue",
            plain_image(),
            vec![],
            MessageType::ItemScored,
            Text::plain("Line scored"),
            true,
        ),
        // Team key, texture and message all non-ASCII; the message has a surrogate pair (U+1F389).
        "non_ascii" => packet(
            0x7FFF_FFFF_FFFF_FFFF_0000_0000_0000_0042,
            JavaDuration { seconds: 0, nanos: 5_000_000 },
            "bingo_茶",
            image(None, Some("tex/茶.png"), ItemStyle::Normal, Decoration::None, None, false),
            vec![],
            MessageType::CardCompleted,
            Text::plain("é 茶 🎉"),
            false,
        ),
        "type_leading_team" | "type_line_scored" | "type_item_scored" | "type_card_completed" => {
            let (message_type, text) = match name {
                "type_leading_team" => (MessageType::LeadingTeam, "type LEADING_TEAM"),
                "type_line_scored" => (MessageType::LineScored, "type LINE_SCORED"),
                "type_item_scored" => (MessageType::ItemScored, "type ITEM_SCORED"),
                _ => (MessageType::CardCompleted, "type CARD_COMPLETED"),
            };
            packet(ORDINARY_ID, zero_duration(), "bingo_red", plain_image(), vec![], message_type, Text::plain(text), false)
        }
        // The high bit of the most significant half is set: the wire carries a negative long.
        "uuid_high_bit" => packet(
            0xFEDC_BA98_7654_3210_0123_4567_89AB_CDEF,
            zero_duration(),
            "",
            plain_image(),
            vec![],
            MessageType::LeadingTeam,
            Text::empty(),
            false,
        ),
        // Duration.ofMillis(Long.MAX_VALUE): seconds 9223372036854775, nanos 807000000.
        "duration_max_millis" => packet(
            0,
            JavaDuration { seconds: 9_223_372_036_854_775, nanos: 807_000_000 },
            "",
            plain_image(),
            vec![],
            MessageType::LeadingTeam,
            Text::empty(),
            false,
        ),
        // Duration.ofMillis(Long.MIN_VALUE): seconds -9223372036854776, nanos 192000000. Its toMillis() is Long.MIN_VALUE.
        "duration_min_millis" => packet(
            0,
            JavaDuration { seconds: -9_223_372_036_854_776, nanos: 192_000_000 },
            "",
            plain_image(),
            vec![],
            MessageType::LeadingTeam,
            Text::empty(),
            false,
        ),
        // Duration.ofSeconds(Long.MAX_VALUE): toMillis() overflows, so 0 is written.
        "duration_overflow" => packet(
            0,
            JavaDuration { seconds: i64::MAX, nanos: 0 },
            "",
            plain_image(),
            vec![],
            MessageType::LeadingTeam,
            Text::empty(),
            false,
        ),
        // An image whose item has custom_name and lore, every decoration field set.
        "image_item_named_lore" => packet(
            ORDINARY_ID,
            JavaDuration { seconds: 0, nanos: 1_000_000 },
            "bingo_red",
            named_image(),
            vec![],
            MessageType::CardCompleted,
            Text::plain("named"),
            false,
        ),
        // An image with no item: texture present, decoration texture absent.
        "image_no_item" => packet(
            ORDINARY_ID,
            JavaDuration { seconds: 0, nanos: 1_000_000 },
            "bingo_red",
            no_item_image(),
            vec![],
            MessageType::CardCompleted,
            Text::plain("no item"),
            false,
        ),
        // The top image is the named sword; the list holds three images: no item, a stone x3 with lore and empty
        // (but present) texture strings, and a free space with no item.
        "list_three" => packet(
            ORDINARY_ID,
            JavaDuration { seconds: 0, nanos: 2_000_000 },
            "bingo_red",
            named_image(),
            vec![
                no_item_image(),
                image(Some(stone_three_with_lore()), Some(""), ItemStyle::AllOf, Decoration::None, Some(""), true),
                image(None, Some("t"), ItemStyle::Normal, Decoration::FreeSpace, Some("d"), false),
            ],
            MessageType::LineScored,
            Text::plain("three"),
            true,
        ),
        // Texture and decoration texture present but empty: present strings, not absent fields.
        "nullables_empty_strings" => packet(
            ORDINARY_ID,
            zero_duration(),
            "",
            image(None, Some(""), ItemStyle::Normal, Decoration::None, Some(""), false),
            vec![],
            MessageType::LeadingTeam,
            Text::empty(),
            false,
        ),
        other => panic!("no case named {other}"),
    }
}

#[test]
fn the_oracle_has_exactly_the_cases_this_file_knows() {
    let names: Vec<String> = rows().into_iter().map(|r| r.name).collect();
    assert_eq!(names, CASES.iter().map(|s| s.to_string()).collect::<Vec<_>>());
}

#[test]
fn every_row_matches_the_mods_bytes_and_channel() {
    for row in rows() {
        assert_eq!(row.channel, ID, "channel of {}", row.name);
        let written = hex_of(&encode(&case(&row.name)));
        assert_eq!(written, row.hex, "bytes of {}", row.name);
    }
}

/// What a row reads back as. Every case reads back as written, except an overflowing duration: `writeDuration` writes 0
/// for it, so the value the reader gets is zero.
fn read_back_value(name: &str) -> GameMessagePacket {
    let mut value = case(name);
    if name == "duration_overflow" {
        value.time_elapsed = zero_duration();
    }
    value
}

#[test]
fn oracle_bytes_read_back_to_the_value_that_was_written() {
    for row in rows() {
        let read = decode(&bytes_of(&row.hex), GameMessagePacket::read);
        assert_eq!(read, Ok(read_back_value(&row.name)), "reading {}", row.name);
    }
}

#[test]
fn the_rows_cover_every_message_type() {
    let mut seen: Vec<MessageType> = rows()
        .iter()
        .map(|row| decode(&bytes_of(&row.hex), GameMessagePacket::read).expect("row decodes").message_type)
        .collect();
    seen.sort_by_key(|t| *t as i32);
    seen.dedup();
    assert_eq!(seen, MessageType::VALUES.to_vec());
}
