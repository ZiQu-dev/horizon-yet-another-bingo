//! Text components and item stacks against the real mod: every row of tests/oracle/w2.tsv was written by
//! the mod's own `PacketBufImpl` (vanilla's component and item stream codecs) over built-in registries
//! (exp-w2/oracle/OracleW2.java). Registry ids (items, data components) come from the same run.
use horizon_bingo::buf::{decode, encode, DecodeError};
use horizon_bingo::item::{component, read_item_stack, write_item_stack, ComponentValue, ItemStack};
use horizon_bingo::nbt::{from_modified_utf8, modified_utf8, read_nbt, write_nbt, Nbt};
use horizon_bingo::text::{read_text, write_text, write_text_or_empty, Text};

const TSV: &str = include_str!("oracle/w2.tsv");

fn row(name: &str) -> Vec<u8> {
    let line = TSV.lines().find(|l| l.split('\t').next() == Some(name)).unwrap_or_else(|| panic!("no row {name}"));
    let hex = line.split('\t').nth(2).unwrap();
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn number(name: &str) -> i32 {
    let line = TSV.lines().find(|l| l.split('\t').next() == Some(name)).unwrap_or_else(|| panic!("no row {name}"));
    line.split('\t').nth(2).unwrap().parse().unwrap()
}

fn text_rows() -> Vec<&'static str> {
    TSV.lines().map(|l| l.split('\t').next().unwrap()).filter(|n| n.starts_with("text_")).collect()
}

fn item_rows() -> Vec<&'static str> {
    TSV.lines().map(|l| l.split('\t').next().unwrap()).filter(|n| n.starts_with("item_") && !n.starts_with("item_id_")).collect()
}

fn written_text(t: &Text) -> Vec<u8> {
    encode(|w| write_text(w, t).unwrap())
}

fn s(v: &str) -> Nbt {
    Nbt::string(v)
}

fn t(v: &str) -> Text {
    Text::plain(v)
}

fn styled(entries: Vec<(&str, Nbt)>) -> Text {
    Text(Nbt::compound(entries))
}

#[test]
fn every_text_the_mod_wrote_decodes_and_encodes_to_the_same_bytes() {
    let rows = text_rows();
    assert_eq!(rows.len(), 22, "the oracle's text cases");
    for name in rows {
        let bytes = row(name);
        let text = decode(&bytes, read_text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(written_text(&text), bytes, "{name}");
    }
}

#[test]
fn texts_built_by_hand_match_the_mods_bytes() {
    // A plain literal is a bare String tag, with Java's modified UTF-8 (the emoji is two 3-byte surrogates).
    assert_eq!(written_text(&Text::empty()), row("text_empty"));
    assert_eq!(written_text(&t("hello")), row("text_hello"));
    assert_eq!(written_text(&t("zażółć 日本 \u{1F600}")), row("text_unicode"));
    // Styled: a compound, entries in the order the mod's codec wrote them.
    assert_eq!(written_text(&styled(vec![("color", s("yellow")), ("text", s("hi"))])), row("text_yellow"));
    assert_eq!(written_text(&styled(vec![("color", s("#FF8800")), ("text", s("hi"))])), row("text_hexcolor"));
    assert_eq!(written_text(&styled(vec![("text", s("b")), ("bold", Nbt::Byte(1))])), row("text_bold"));
    assert_eq!(written_text(&styled(vec![("text", s("b")), ("bold", Nbt::Byte(0))])), row("text_notbold"));
    assert_eq!(written_text(&styled(vec![("underlined", Nbt::Byte(1)), ("text", s("u"))])), row("text_underlined"));
    assert_eq!(written_text(&styled(vec![("insertion", s("ins")), ("text", s("x"))])), row("text_insertion"));
    // Siblings are an `extra` list; an unstyled sibling is a bare String, so the list's element type is 8.
    assert_eq!(
        written_text(&styled(vec![("extra", Nbt::list(vec![s("b")])), ("text", s("a"))])),
        row("text_extra1")
    );
    assert_eq!(written_text(&styled(vec![("translate", s("bingo.tier.c"))])), row("text_translate"));
    assert_eq!(written_text(&styled(vec![("keybind", s("key.inventory"))])), row("text_keybind"));
}

#[test]
fn a_missing_text_is_the_empty_component_not_an_absent_field() {
    // `writeText(null)` in PacketBufImpl encodes `Component.empty()`.
    let bytes = encode(|w| write_text_or_empty(w, None).unwrap());
    assert_eq!(bytes, row("text_empty"));
    assert_eq!(bytes, [0x08, 0x00, 0x00]);
}

#[test]
fn modified_utf8_matches_java() {
    assert_eq!(modified_utf8("\u{0}"), [0xc0, 0x80], "NUL is two bytes");
    assert_eq!(modified_utf8("\u{1F600}"), [0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80], "a supplementary character is a surrogate pair");
    assert_eq!(from_modified_utf8(&[0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80]).unwrap(), "\u{1F600}");
    assert_eq!(from_modified_utf8(&[0xc0, 0x80]).unwrap(), "\u{0}");
    // A lone surrogate cannot be a Rust string.
    assert_eq!(from_modified_utf8(&[0xed, 0xa0, 0xbd]), Err(DecodeError::InvalidModifiedUtf8));
    // A continuation byte with nothing before it.
    assert_eq!(from_modified_utf8(&[0x80]), Err(DecodeError::InvalidModifiedUtf8));
}

#[test]
fn nbt_limits() {
    // 600 nested lists exceed vanilla's depth of 512.
    let mut deep = Vec::new();
    deep.push(9u8);
    for _ in 0..600 {
        deep.extend_from_slice(&[9, 0, 0, 0, 1]);
    }
    deep.extend_from_slice(&[0, 0, 0, 0, 0]);
    assert_eq!(decode(&deep, read_nbt), Err(DecodeError::NbtTooDeep));
    // An End tag is not a value; an unknown tag id is rejected; a length past the buffer fails without allocating.
    assert_eq!(decode(&[0], read_nbt), Err(DecodeError::InvalidNbtTag(0)));
    assert_eq!(decode(&[13], read_nbt), Err(DecodeError::InvalidNbtTag(13)));
    assert!(decode(&[7, 0x7f, 0xff, 0xff, 0xff, 1], read_nbt).is_err());
    // A string whose modified-UTF-8 form is over 65535 bytes cannot be written (Java's writeUTF throws too).
    let long = "a".repeat(65536);
    let mut w = horizon_bingo::buf::Writer::default();
    assert_eq!(write_nbt(&mut w, &Nbt::String(long)), Err(DecodeError::InvalidModifiedUtf8));
}

fn id(name: &str) -> i32 {
    number(&format!("item_id_{name}"))
}

fn stack(count: i32, item: i32, added: Vec<(i32, ComponentValue)>, removed: Vec<i32>) -> Option<ItemStack> {
    Some(ItemStack { count, item, added, removed })
}

fn ct(text: Text) -> ComponentValue {
    ComponentValue::Text(text)
}

fn written_item(s: Option<&ItemStack>) -> Vec<u8> {
    encode(|w| write_item_stack(w, s).unwrap())
}

#[test]
fn component_and_item_ids_are_the_real_registry_ids() {
    assert_eq!(component::CUSTOM_DATA, number("component_id_CUSTOM_DATA"));
    assert_eq!(component::MAX_STACK_SIZE, number("component_id_MAX_STACK_SIZE"));
    assert_eq!(component::MAX_DAMAGE, number("component_id_MAX_DAMAGE"));
    assert_eq!(component::DAMAGE, number("component_id_DAMAGE"));
    assert_eq!(component::CUSTOM_NAME, number("component_id_CUSTOM_NAME"));
    assert_eq!(component::ITEM_NAME, number("component_id_ITEM_NAME"));
    assert_eq!(component::ITEM_MODEL, number("component_id_ITEM_MODEL"));
    assert_eq!(component::LORE, number("component_id_LORE"));
    assert_eq!(component::ENCHANTMENTS, number("component_id_ENCHANTMENTS"));
    assert_eq!(component::ATTRIBUTE_MODIFIERS, number("component_id_ATTRIBUTE_MODIFIERS"));
    assert_eq!(component::TOOLTIP_DISPLAY, number("component_id_TOOLTIP_DISPLAY"));
    assert_eq!(component::BASE_COLOR, number("component_id_BASE_COLOR"));
}

#[test]
fn every_item_the_mod_wrote_decodes_and_encodes_to_the_same_bytes() {
    let rows = item_rows();
    assert_eq!(rows.len(), 20, "the oracle's item cases");
    for name in rows {
        let bytes = row(name);
        let item = decode(&bytes, read_item_stack).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(written_item(item.as_ref()), bytes, "{name}");
    }
}

#[test]
fn items_built_by_hand_match_the_mods_bytes() {
    let stone = id("minecraft:stone");
    let sword = id("minecraft:diamond_sword");
    assert_eq!(written_item(None), row("item_empty"));
    assert_eq!(written_item(stack(1, stone, vec![], vec![]).as_ref()), row("item_stone1"));
    assert_eq!(written_item(stack(64, stone, vec![], vec![]).as_ref()), row("item_stone64"));
    assert_eq!(written_item(stack(99, stone, vec![], vec![]).as_ref()), row("item_stack99"));
    assert_eq!(
        written_item(stack(1, id("minecraft:spruce_sapling"), vec![], vec![]).as_ref()),
        row("item_sapling")
    );
    assert_eq!(
        written_item(stack(1, sword, vec![(component::CUSTOM_NAME, ct(t("Excalibur")))], vec![]).as_ref()),
        row("item_custom_name")
    );
    assert_eq!(
        written_item(stack(1, stone, vec![(component::ITEM_NAME, ct(t("Named stone")))], vec![]).as_ref()),
        row("item_item_name")
    );
    let italic = styled(vec![("text", s("line2")), ("italic", Nbt::Byte(1))]);
    assert_eq!(
        written_item(stack(1, stone, vec![(component::LORE, ComponentValue::TextList(vec![t("line1"), italic]))], vec![]).as_ref()),
        row("item_lore2")
    );
    assert_eq!(
        written_item(
            stack(3, stone, vec![(component::CUSTOM_NAME, ct(t("N"))), (component::LORE, ComponentValue::TextList(vec![t("L")]))], vec![])
                .as_ref()
        ),
        row("item_name_and_lore")
    );
    assert_eq!(written_item(stack(1, sword, vec![(component::DAMAGE, ComponentValue::VarInt(5))], vec![]).as_ref()), row("item_damage5"));
    assert_eq!(written_item(stack(1, sword, vec![], vec![component::MAX_DAMAGE]).as_ref()), row("item_removed_max_damage"));
    assert_eq!(
        written_item(stack(1, stone, vec![(component::ITEM_MODEL, ComponentValue::Identifier("minecraft:diamond".into()))], vec![]).as_ref()),
        row("item_item_model")
    );
    assert_eq!(
        written_item(
            stack(1, stone, vec![(component::CUSTOM_DATA, ComponentValue::Nbt(Nbt::compound(vec![("s", s("ab")), ("n", Nbt::Int(7))])))], vec![])
                .as_ref()
        ),
        row("item_custom_data")
    );
    let tooltip = |hide: bool, hidden: Vec<i32>| vec![(component::TOOLTIP_DISPLAY, ComponentValue::TooltipDisplay { hide, hidden })];
    assert_eq!(
        written_item(stack(1, stone, tooltip(true, vec![component::ENCHANTMENTS, component::ATTRIBUTE_MODIFIERS]), vec![]).as_ref()),
        row("item_tooltip_hide_two")
    );
    assert_eq!(written_item(stack(1, stone, tooltip(false, vec![]), vec![]).as_ref()), row("item_tooltip_default_like"));
    assert_eq!(written_item(stack(1, stone, tooltip(false, vec![component::LORE]), vec![]).as_ref()), row("item_tooltip_hide_lore"));
    // A banner-style dye colour: the DyeColor's id (white 0, red 14, black 15).
    let shield = id("minecraft:shield");
    for (name, dye) in [("item_base_color_white", 0), ("item_base_color_red", 14), ("item_base_color_black", 15)] {
        assert_eq!(written_item(stack(1, shield, vec![(component::BASE_COLOR, ComponentValue::VarInt(dye))], vec![]).as_ref()), row(name), "{name}");
    }
    // Added and removed together; a damage of 300 is a two-byte VarInt (`ac 02`).
    assert_eq!(
        written_item(
            stack(1, sword, vec![(component::ITEM_NAME, ct(t("Blade"))), (component::DAMAGE, ComponentValue::VarInt(300))], vec![component::MAX_DAMAGE])
                .as_ref()
        ),
        row("item_added_and_removed")
    );
}

#[test]
fn a_component_type_this_crate_cannot_read_is_reported_not_skipped() {
    // count 1, item 1, 1 added, 0 removed, component type 5, then bytes whose meaning we do not know.
    let bytes = [1u8, 1, 1, 0, 5, 0xaa, 0xbb];
    assert_eq!(decode(&bytes, read_item_stack), Err(DecodeError::UnsupportedComponent(5)));
    // And one it can read but was given the wrong value for is a write error, not silent garbage.
    let wrong = ItemStack { count: 1, item: 1, added: vec![(component::DAMAGE, ComponentValue::Text(Text::empty()))], removed: vec![] };
    assert!(encode_result(&wrong).is_err());
}

fn encode_result(stack: &ItemStack) -> horizon_bingo::buf::Result<()> {
    let mut w = horizon_bingo::buf::Writer::default();
    write_item_stack(&mut w, Some(stack))
}
