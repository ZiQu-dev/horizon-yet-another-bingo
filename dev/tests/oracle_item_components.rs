//! Item components beyond the first set, against the real game. Every row of tests/oracle/item_components.tsv is the
//! bytes that vanilla 26.3's own `ItemStack.OPTIONAL_STREAM_CODEC` wrote for one stack, over the game's registries
//! (the oracle is ~/bingo-horizon-port/exp-oracle3/src/OracleItems.java). Its registry ids are in
//! tests/oracle/data_component_ids.tsv (data components), tests/oracle/holder_ids.tsv (holders and items). The expected
//! fields below are the oracle's inputs, not this crate's decoder output.
//!
//! Regenerate (in the oracle folder; it needs Java 26 and the jars listed in cp.txt):
//!   javac -cp "$(cat cp.txt)" -d out src/OracleItems.java
//!   java -cp "out:$(cat cp.txt)" OracleItems > run.raw 2>/dev/null     # the rows, prefixed "ROW "
//!   java -cp "out:$(cat cp.txt)" OracleItems components                 # id table, prefixed "COMP "
//!   java -cp "out:$(cat cp.txt)" OracleItems holders                    # holder and item ids, prefixed "HOLDER " / "ITEM "
//! The logger prefixes each line with a timestamp and "[STDOUT]: "; strip that before using the rows.
use horizon_bingo::buf::{decode, encode, DecodeError};
use horizon_bingo::item::{
    component, read_item_stack, write_item_stack, AttributeDisplay, AttributeModifier, ComponentValue, EnchantmentLevel,
    Filterable, FireworkExplosion, GameProfile, ItemStack, MobEffect, MobEffectDetails, PartialProfile, PotionContents,
    ProfileProperty, ProfileSource, ResolvableProfile, SkinPatch, WrittenBookContent,
};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;

const ROWS: &str = include_str!("oracle/item_components.tsv");
const COMPONENTS: &str = include_str!("oracle/data_component_ids.tsv");
const HOLDERS: &str = include_str!("oracle/holder_ids.tsv");

fn row_fields(name: &str) -> Option<Vec<&'static str>> {
    ROWS.lines().map(|l| l.split('\t').collect::<Vec<_>>()).find(|f| f[0] == name)
}

fn row(name: &str) -> Vec<u8> {
    hex_bytes(row_fields(name).unwrap_or_else(|| panic!("no row {name}"))[2])
}

fn row_names() -> Vec<&'static str> {
    ROWS.lines().map(|l| l.split('\t').next().unwrap()).collect()
}

/// A data component's registry id, by its name without the namespace.
fn component_id(name: &str) -> i32 {
    let full = format!("minecraft:{name}");
    let line = COMPONENTS.lines().find(|l| l.split('\t').nth(1) == Some(full.as_str())).unwrap_or_else(|| panic!("no component {full}"));
    line.split('\t').next().unwrap().parse().unwrap()
}

/// A registry id of the holder table (`HOLDER <registry> <name> <id>` or `ITEM <name> <id>`), by its key.
fn holder_id(kind: &str, name: &str) -> i32 {
    let line = HOLDERS
        .lines()
        .find(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            f.len() == 3 && f[0] == kind && f[1] == name
        })
        .unwrap_or_else(|| panic!("no {kind} {name}"));
    line.split('\t').nth(2).unwrap().parse().unwrap()
}

fn item(name: &str) -> i32 {
    holder_id("ITEM", name)
}

fn potion(name: &str) -> i32 {
    holder_id("potion", name)
}

fn effect(name: &str) -> i32 {
    holder_id("mob_effect", name)
}

fn enchantment(name: &str) -> i32 {
    holder_id("enchantment", name)
}

fn attribute(name: &str) -> i32 {
    holder_id("attribute", name)
}

fn read(name: &str) -> ItemStack {
    decode(&row(name), read_item_stack).unwrap_or_else(|e| panic!("{name}: {e}")).unwrap_or_else(|| panic!("{name} is empty"))
}

fn written(stack: &ItemStack) -> Vec<u8> {
    encode(|w| write_item_stack(w, Some(stack)).unwrap())
}

fn stack(count: i32, item: i32, added: Vec<(i32, ComponentValue)>) -> ItemStack {
    ItemStack { count, item, added, removed: vec![] }
}

fn details(amplifier: i32, duration: i32, ambient: bool, visible: bool, show_icon: bool) -> MobEffectDetails {
    MobEffectDetails { amplifier, duration, ambient, visible, show_icon, hidden: None }
}

fn firework(shape: i32, colors: Vec<i32>, fade_colors: Vec<i32>, has_trail: bool, has_twinkle: bool) -> FireworkExplosion {
    FireworkExplosion { shape, colors, fade_colors, has_trail, has_twinkle }
}

#[test]
fn every_row_decodes_and_encodes_to_the_same_bytes() {
    let names = row_names();
    assert_eq!(names.len(), 53, "the oracle's rows");
    // The one row this crate refuses (tooltip style, a component with no codec here) has its own test below, so no
    // row is skipped silently.
    let (unsupported, supported): (Vec<&str>, Vec<&str>) = names.into_iter().partition(|n| n.starts_with("unsupported_"));
    assert_eq!(unsupported, vec!["unsupported_tooltip_style"]);
    for name in supported {
        let bytes = row(name);
        let decoded = decode(&bytes, read_item_stack).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(encode(|w| write_item_stack(w, decoded.as_ref()).unwrap()), bytes, "{name}");
    }
}

#[test]
fn component_ids_are_the_real_registry_ids() {
    assert_eq!(component::UNBREAKABLE, component_id("unbreakable"));
    assert_eq!(component::RARITY, component_id("rarity"));
    assert_eq!(component::ENCHANTMENTS, component_id("enchantments"));
    assert_eq!(component::ATTRIBUTE_MODIFIERS, component_id("attribute_modifiers"));
    assert_eq!(component::CUSTOM_MODEL_DATA, component_id("custom_model_data"));
    assert_eq!(component::REPAIR_COST, component_id("repair_cost"));
    assert_eq!(component::STORED_ENCHANTMENTS, component_id("stored_enchantments"));
    assert_eq!(component::DYED_COLOR, component_id("dyed_color"));
    assert_eq!(component::MAP_ID, component_id("map_id"));
    assert_eq!(component::CHARGED_PROJECTILES, component_id("charged_projectiles"));
    assert_eq!(component::BUNDLE_CONTENTS, component_id("bundle_contents"));
    assert_eq!(component::POTION_CONTENTS, component_id("potion_contents"));
    assert_eq!(component::TRIM, component_id("trim"));
    assert_eq!(component::INSTRUMENT, component_id("instrument"));
    assert_eq!(component::OMINOUS_BOTTLE_AMPLIFIER, component_id("ominous_bottle_amplifier"));
    assert_eq!(component::JUKEBOX_PLAYABLE, component_id("jukebox_playable"));
    assert_eq!(component::FIREWORK_EXPLOSION, component_id("firework_explosion"));
    assert_eq!(component::FIREWORKS, component_id("fireworks"));
    assert_eq!(component::CONTAINER, component_id("container"));
    assert_eq!(component::PROFILE, component_id("profile"));
    assert_eq!(component::WRITTEN_BOOK_CONTENT, component_id("written_book_content"));
}

#[test]
fn potions_and_mob_effects() {
    let healing = read("potion_healing");
    assert_eq!(healing.added, vec![(component::POTION_CONTENTS, ComponentValue::PotionContents(PotionContents {
        potion: Some(potion("healing")),
        custom_color: None,
        effects: vec![],
        custom_name: None,
    }))]);
    assert_eq!(written(&healing), row("potion_healing"));

    let full = read("potion_full");
    assert_eq!(full.item, item("potion"));
    assert_eq!(full.added, vec![(component::POTION_CONTENTS, ComponentValue::PotionContents(PotionContents {
        potion: Some(potion("swiftness")),
        custom_color: Some(0x3399FF),
        effects: vec![MobEffect { effect: effect("speed"), details: details(1, 600, false, true, true) }],
        custom_name: Some("Fancy".into()),
    }))]);
    assert_eq!(written(&full), row("potion_full"));

    let two = read("potion_two_effects");
    assert_eq!(two.item, item("splash_potion"));
    let ComponentValue::PotionContents(p) = &two.added[0].1 else { panic!("not a potion") };
    assert_eq!(p.effects, vec![
        MobEffect { effect: effect("speed"), details: details(0, 100, false, true, true) },
        MobEffect { effect: effect("regeneration"), details: details(2, 2000, false, true, true) },
    ]);

    let ambient = read("effect_ambient_noicon");
    let ComponentValue::PotionContents(p) = &ambient.added[0].1 else { panic!("not a potion") };
    assert_eq!(p.effects, vec![MobEffect { effect: effect("strength"), details: details(2, 3600, true, false, false) }]);

    let custom = read("potion_custom_color");
    let ComponentValue::PotionContents(p) = &custom.added[0].1 else { panic!("not a potion") };
    assert_eq!((p.potion, p.custom_color), (None, Some(0x00FF00)));
    assert_eq!(written(&custom), row("potion_custom_color"));
}

#[test]
fn a_hidden_effect_is_written_without_its_holder() {
    // The potion's effect carries no hidden effect on the wire, so the flag is false, and the nested chain is read in
    // the loop the reader uses. The hidden flag and nested details are what vanilla writes for a standalone effect too.
    let hidden = read("effect_hidden");
    let ComponentValue::PotionContents(p) = &hidden.added[0].1 else { panic!("not a potion") };
    assert_eq!(p.effects[0].details, details(0, 100, false, true, true));
    assert_eq!(written(&hidden), row("effect_hidden"));
}

/// The standalone `MobEffectInstance` codec's bytes for speed (holder 0) with a hidden regeneration effect, from the
/// oracle's `effects` mode (`EFF_HIDDEN`). A potion's own effect list never writes a hidden effect, so this effect is
/// spliced into a potion stack whose other fields are the oracle's `effect_hidden` row: the reader must take the hidden
/// chain as this codec writes it.
const EFFECT_WITH_HIDDEN: &str = "00006400010101013200010100";

#[test]
fn a_hidden_effect_chain_is_read_as_the_standalone_codec_writes_it() {
    let mut bytes = hex_bytes("01f709010035000001");
    bytes.extend(hex_bytes(EFFECT_WITH_HIDDEN));
    bytes.push(0);
    let stack = decode(&bytes, read_item_stack).unwrap().unwrap();
    let ComponentValue::PotionContents(p) = &stack.added[0].1 else { panic!("not a potion") };
    let hidden = p.effects[0].details.hidden.as_deref().expect("a hidden effect");
    assert_eq!(hidden, &details(1, 50, false, true, true));
    assert_eq!(hidden.hidden, None);
    assert_eq!(written(&stack), bytes);
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn sorted_by_enchantment(mut entries: Vec<EnchantmentLevel>) -> Vec<EnchantmentLevel> {
    entries.sort_by_key(|e| e.enchantment);
    entries
}

fn enchantments_of(stack: &ItemStack) -> Vec<EnchantmentLevel> {
    match &stack.added[0].1 {
        ComponentValue::Enchantments(entries) => entries.clone(),
        other => panic!("not enchantments: {other:?}"),
    }
}

#[test]
fn enchantments_are_read_with_their_levels() {
    // The encoder orders the entries by its own map iteration, so the field checks compare sets (sorted by id).
    let one = read("ench_sharpness5");
    assert_eq!(one.added, vec![(component::ENCHANTMENTS, ComponentValue::Enchantments(vec![
        EnchantmentLevel { enchantment: enchantment("sharpness"), level: 5 },
    ]))]);
    assert_eq!(written(&one), row("ench_sharpness5"));

    let three = read("ench_three");
    assert_eq!(sorted_by_enchantment(enchantments_of(&three)), sorted_by_enchantment(vec![
        EnchantmentLevel { enchantment: enchantment("mending"), level: 1 },
        EnchantmentLevel { enchantment: enchantment("sharpness"), level: 5 },
        EnchantmentLevel { enchantment: enchantment("unbreaking"), level: 3 },
    ]));
    assert_eq!(written(&three), row("ench_three"));

    let stored = read("stored_two");
    assert_eq!(stored.item, item("enchanted_book"));
    assert_eq!(stored.added[0].0, component::STORED_ENCHANTMENTS);
    assert_eq!(sorted_by_enchantment(enchantments_of(&stored)), sorted_by_enchantment(vec![
        EnchantmentLevel { enchantment: enchantment("mending"), level: 1 },
        EnchantmentLevel { enchantment: enchantment("fortune"), level: 2 },
    ]));
    assert_eq!(written(&stored), row("stored_two"));
}

#[test]
fn enchantment_entries_keep_the_order_they_are_written_in() {
    // Whatever order the game wrote, the reader keeps it, so a reversed list comes back reversed.
    let mut reversed = read("ench_three");
    let ComponentValue::Enchantments(entries) = &mut reversed.added[0].1 else { panic!("not enchantments") };
    entries.reverse();
    let expected = entries.clone();
    let bytes = written(&reversed);
    let back = decode(&bytes, read_item_stack).unwrap().unwrap();
    assert_eq!(enchantments_of(&back), expected);
}

#[test]
fn dyed_colour_is_four_bytes_without_a_flag() {
    assert_eq!(read("dyed_blue").added, vec![(component::DYED_COLOR, ComponentValue::Int(0x3366FF))]);
    assert_eq!(read("dyed_zero").added, vec![(component::DYED_COLOR, ComponentValue::Int(0))]);
    // The top bit is a sign bit on the wire (a big-endian Java int).
    assert_eq!(read("dyed_negative").added, vec![(component::DYED_COLOR, ComponentValue::Int(0xFF112233u32 as i32))]);
    assert_eq!(written(&read("dyed_negative")), row("dyed_negative"));
}

#[test]
fn fireworks_and_their_explosions() {
    let star = firework(2, vec![0xFF0000, 0x00FF00], vec![0x0000FF], true, false);
    let plain = firework(0, vec![], vec![], false, false);
    let burst = firework(4, vec![0xFFFFFF], vec![], false, true);
    assert_eq!(read("firework_star").added, vec![(component::FIREWORK_EXPLOSION, ComponentValue::FireworkExplosion(star.clone()))]);
    assert_eq!(read("firework_plain").added, vec![(component::FIREWORK_EXPLOSION, ComponentValue::FireworkExplosion(plain))]);
    assert_eq!(read("firework_burst_twinkle").added, vec![(component::FIREWORK_EXPLOSION, ComponentValue::FireworkExplosion(burst.clone()))]);
    assert_eq!(read("fireworks_two").added, vec![(component::FIREWORKS, ComponentValue::Fireworks {
        flight_duration: 2,
        explosions: vec![star.clone(), burst],
    })]);
    assert_eq!(read("fireworks_empty").added, vec![(component::FIREWORKS, ComponentValue::Fireworks { flight_duration: 0, explosions: vec![] })]);
    let built = stack(1, item("firework_star"), vec![(component::FIREWORK_EXPLOSION, ComponentValue::FireworkExplosion(star))]);
    assert_eq!(written(&built), row("firework_star"));
}

#[test]
fn custom_model_data_lists() {
    let all = read("cmd_all");
    assert_eq!(all.added, vec![(component::CUSTOM_MODEL_DATA, ComponentValue::CustomModelData {
        floats: vec![1.5, -2.0],
        flags: vec![true, false],
        strings: vec!["abc".into()],
        colors: vec![0x112233],
    })]);
    assert_eq!(written(&all), row("cmd_all"));
    assert_eq!(read("cmd_empty_value").added, vec![(component::CUSTOM_MODEL_DATA, ComponentValue::CustomModelData {
        floats: vec![], flags: vec![], strings: vec![], colors: vec![],
    })]);
}

#[test]
fn registry_holders_are_sent_as_their_id_plus_one() {
    // trim material and pattern, instrument and jukebox song: the wire holds id + 1 (see the rows, which have 5 for gold).
    assert_eq!(read("trim_gold_sentry").added, vec![(component::TRIM, ComponentValue::Trim {
        material: holder_id("trim_material", "gold"),
        pattern: holder_id("trim_pattern", "sentry"),
    })]);
    assert_eq!(written(&read("trim_gold_sentry")), row("trim_gold_sentry"));
    assert_eq!(read("instrument_sing").added, vec![(component::INSTRUMENT, ComponentValue::RegistryId(holder_id("instrument", "sing_goat_horn")))]);
    assert_eq!(read("jukebox_cat").added, vec![(component::JUKEBOX_PLAYABLE, ComponentValue::RegistryId(holder_id("jukebox_song", "cat")))]);
    assert_eq!(written(&read("instrument_sing")), row("instrument_sing"));
    assert_eq!(written(&read("jukebox_cat")), row("jukebox_cat"));
}

#[test]
fn small_values() {
    assert_eq!(read("map_id_7").added, vec![(component::MAP_ID, ComponentValue::VarInt(7))]);
    assert_eq!(read("map_id_neg").added, vec![(component::MAP_ID, ComponentValue::VarInt(-1))]);
    assert_eq!(read("ominous_2").added, vec![(component::OMINOUS_BOTTLE_AMPLIFIER, ComponentValue::VarInt(2))]);
    assert_eq!(read("repair_3").added, vec![(component::REPAIR_COST, ComponentValue::VarInt(3))]);
    // Rarity is the enum's ordinal: EPIC is 3.
    assert_eq!(read("rarity_epic").added, vec![(component::RARITY, ComponentValue::VarInt(3))]);
    // Unbreakable is a marker with no value.
    assert_eq!(read("unbreakable").added, vec![(component::UNBREAKABLE, ComponentValue::Unit)]);
    for name in ["map_id_7", "map_id_neg", "ominous_2", "repair_3", "rarity_epic", "unbreakable"] {
        assert_eq!(written(&read(name)), row(name), "{name}");
    }
}

#[test]
fn containers_hold_templates_and_empty_slots() {
    let chest = read("container_three");
    assert_eq!(chest.added, vec![(component::CONTAINER, ComponentValue::Container(vec![
        Some(stack(64, item("stone"), vec![])),
        None,
        Some(stack(1, item("diamond_sword"), vec![(component::DAMAGE, ComponentValue::VarInt(5))])),
    ]))]);
    assert_eq!(written(&chest), row("container_three"));

    let bundle = read("bundle_two");
    assert_eq!(bundle.added, vec![(component::BUNDLE_CONTENTS, ComponentValue::Stacks(vec![
        stack(3, item("stone"), vec![]),
        stack(1, item("diamond"), vec![]),
    ]))]);
    assert_eq!(written(&bundle), row("bundle_two"));

    let crossbow = read("crossbow_arrow");
    assert_eq!(crossbow.added, vec![(component::CHARGED_PROJECTILES, ComponentValue::Stacks(vec![stack(1, item("arrow"), vec![])]))]);
    assert_eq!(written(&crossbow), row("crossbow_arrow"));
}

#[test]
fn attribute_modifiers_and_their_displays() {
    let two = read("attr_two");
    assert_eq!(two.added, vec![(component::ATTRIBUTE_MODIFIERS, ComponentValue::AttributeModifiers(vec![
        AttributeModifier {
            attribute: attribute("attack_damage"),
            id: "minecraft:base_attack_damage".into(),
            amount: 5.0,
            operation: 0,
            slot_group: 1,
            display: AttributeDisplay::Default,
        },
        AttributeModifier {
            attribute: attribute("attack_speed"),
            id: "minecraft:base_attack_speed".into(),
            amount: -0.5,
            operation: 2,
            slot_group: 8,
            display: AttributeDisplay::Default,
        },
    ]))]);
    assert_eq!(written(&two), row("attr_two"));

    let hidden = read("attr_hidden");
    assert_eq!(hidden.added, vec![(component::ATTRIBUTE_MODIFIERS, ComponentValue::AttributeModifiers(vec![AttributeModifier {
        attribute: attribute("attack_damage"),
        id: "minecraft:hidden_one".into(),
        amount: 1.0,
        operation: 0,
        slot_group: 7,
        display: AttributeDisplay::Hidden,
    }]))]);
    assert_eq!(written(&hidden), row("attr_hidden"));

    let named = read("attr_override");
    assert_eq!(named.added, vec![(component::ATTRIBUTE_MODIFIERS, ComponentValue::AttributeModifiers(vec![AttributeModifier {
        attribute: attribute("attack_damage"),
        id: "minecraft:named".into(),
        amount: 2.0,
        operation: 1,
        slot_group: 0,
        display: AttributeDisplay::Override(Text::plain("Named")),
    }]))]);
    assert_eq!(written(&named), row("attr_override"));
}

#[test]
fn stacks_built_by_hand_match_the_games_bytes() {
    let burst = firework(4, vec![0xFFFFFF], vec![], false, true);
    let red = ComponentValue::PotionContents(PotionContents {
        potion: Some(potion("swiftness")),
        custom_color: Some(0x3399FF),
        effects: vec![MobEffect { effect: effect("speed"), details: details(1, 600, false, true, true) }],
        custom_name: Some("Fancy".into()),
    });
    assert_eq!(written(&stack(1, item("potion"), vec![(component::POTION_CONTENTS, red)])), row("potion_full"));
    assert_eq!(
        written(&stack(1, item("firework_rocket"), vec![(component::FIREWORKS, ComponentValue::Fireworks {
            flight_duration: 2,
            explosions: vec![firework(2, vec![0xFF0000, 0x00FF00], vec![0x0000FF], true, false), burst],
        })])),
        row("fireworks_two")
    );
    assert_eq!(
        written(&stack(1, item("iron_sword"), vec![(component::REPAIR_COST, ComponentValue::VarInt(3))])),
        row("repair_3")
    );
}

#[test]
fn components_outside_the_readable_set_are_still_refused() {
    // Tooltip style (35) has a codec this crate does not model, so its stack is refused rather than skipped.
    let style = row("unsupported_tooltip_style");
    assert_eq!(decode(&style, read_item_stack), Err(DecodeError::UnsupportedComponent(component_id("tooltip_style"))));
}

const NOTCH: u128 = 0x0123456789abcdef_fedcba9876543210;

fn no_skin() -> SkinPatch {
    SkinPatch { body: None, cape: None, elytra: None, model: None }
}

fn profile_of(name: &str) -> ResolvableProfile {
    let stack = read(name);
    assert_eq!(stack.item, item("player_head"));
    match stack.added.into_iter().next() {
        Some((id, ComponentValue::Profile(profile))) if id == component::PROFILE => profile,
        other => panic!("not a profile: {other:?}"),
    }
}

#[test]
fn unresolved_profiles_carry_a_name_or_a_uuid() {
    assert_eq!(profile_of("profile_name"), ResolvableProfile {
        source: ProfileSource::Partial(PartialProfile { name: Some("Steve".into()), id: None, properties: vec![] }),
        skin: no_skin(),
    });
    assert_eq!(profile_of("profile_uuid"), ResolvableProfile {
        source: ProfileSource::Partial(PartialProfile { name: None, id: Some(NOTCH), properties: vec![] }),
        skin: no_skin(),
    });
}

#[test]
fn resolved_profiles_keep_their_uuid_name_and_properties_in_order() {
    assert_eq!(profile_of("profile_resolved_bare"), ResolvableProfile {
        source: ProfileSource::Resolved(GameProfile { id: NOTCH, name: "Notch".into(), properties: vec![] }),
        skin: no_skin(),
    });
    assert_eq!(profile_of("profile_resolved_textures"), ResolvableProfile {
        source: ProfileSource::Resolved(GameProfile {
            id: NOTCH,
            name: "Notch".into(),
            properties: vec![ProfileProperty {
                name: "textures".into(),
                value: "eyJ0ZXh0dXJlcyI6e319".into(),
                signature: Some("c2lnbmF0dXJl".into()),
            }],
        }),
        skin: no_skin(),
    });
    // Two properties, the first without a signature: the order is the map's, and each signature is its own flag.
    assert_eq!(profile_of("profile_resolved_two_props"), ResolvableProfile {
        source: ProfileSource::Resolved(GameProfile {
            id: NOTCH,
            name: "Steve".into(),
            properties: vec![
                ProfileProperty { name: "alpha".into(), value: "1".into(), signature: None },
                ProfileProperty { name: "beta".into(), value: "2".into(), signature: Some("sig".into()) },
            ],
        }),
        skin: no_skin(),
    });
}

#[test]
fn a_skin_patch_is_read_as_identifiers_and_a_model_ordinal() {
    // The body and elytra are identifiers only (the texture paths are derived from them); the model is an ordinal:
    // wide is 0 and slim is 1.
    assert_eq!(profile_of("profile_resolved_skin").skin, SkinPatch {
        body: Some("minecraft:skin/body".into()),
        cape: None,
        elytra: Some("minecraft:skin/elytra".into()),
        model: Some(1),
    });
    assert_eq!(profile_of("profile_unresolved_skin").skin, SkinPatch { body: None, cape: None, elytra: None, model: Some(0) });
}

#[test]
fn a_profile_built_by_hand_matches_the_games_bytes() {
    let textures = ResolvableProfile {
        source: ProfileSource::Resolved(GameProfile {
            id: NOTCH,
            name: "Notch".into(),
            properties: vec![ProfileProperty {
                name: "textures".into(),
                value: "eyJ0ZXh0dXJlcyI6e319".into(),
                signature: Some("c2lnbmF0dXJl".into()),
            }],
        }),
        skin: no_skin(),
    };
    assert_eq!(written(&stack(1, item("player_head"), vec![(component::PROFILE, ComponentValue::Profile(textures))])), row("profile_resolved_textures"));
}

fn book_of(name: &str) -> WrittenBookContent {
    let stack = read(name);
    assert_eq!(stack.item, item("written_book"));
    match stack.added.into_iter().next() {
        Some((id, ComponentValue::WrittenBook(book))) if id == component::WRITTEN_BOOK_CONTENT => book,
        other => panic!("not a book: {other:?}"),
    }
}

#[test]
fn written_books_keep_title_author_generation_and_pages() {
    assert_eq!(book_of("book_two_pages"), WrittenBookContent {
        title: Filterable { raw: "Bingo".into(), filtered: None },
        author: "Horizon".into(),
        generation: 0,
        pages: vec![
            Filterable { raw: Text::plain("Page one"), filtered: None },
            Filterable { raw: Text::plain("Page two"), filtered: None },
        ],
        resolved: true,
    });
    assert_eq!(book_of("book_empty"), WrittenBookContent {
        title: Filterable { raw: String::new(), filtered: None },
        author: String::new(),
        generation: 0,
        pages: vec![],
        resolved: false,
    });
    // A filtered title and page carry both versions; the filtered page is the one a filtered viewer sees.
    assert_eq!(book_of("book_filtered"), WrittenBookContent {
        title: Filterable { raw: "Raw title".into(), filtered: Some("Filtered".into()) },
        author: "Author".into(),
        generation: 3,
        pages: vec![Filterable { raw: Text::plain("raw page"), filtered: Some(Text::plain("filtered page")) }],
        resolved: false,
    });
}

#[test]
fn a_styled_book_page_is_a_compound_as_the_game_wrote_it() {
    let styled = book_of("book_styled_page");
    assert_eq!(styled.generation, 1);
    let page = &styled.pages[0].raw;
    assert_eq!(page.as_plain(), None);
    assert_eq!(page.0.get("text"), Some(&Nbt::String("Gold".into())));
    assert_eq!(page.0.get("color"), Some(&Nbt::String("#FFAA00".into())));
    assert_eq!(page.0.get("bold"), Some(&Nbt::Byte(1)));
}

#[test]
fn a_book_built_by_hand_matches_the_games_bytes() {
    let book = WrittenBookContent {
        title: Filterable { raw: "Bingo".into(), filtered: None },
        author: "Horizon".into(),
        generation: 0,
        pages: vec![
            Filterable { raw: Text::plain("Page one"), filtered: None },
            Filterable { raw: Text::plain("Page two"), filtered: None },
        ],
        resolved: true,
    };
    assert_eq!(written(&stack(1, item("written_book"), vec![(component::WRITTEN_BOOK_CONTENT, ComponentValue::WrittenBook(book))])), row("book_two_pages"));
}
