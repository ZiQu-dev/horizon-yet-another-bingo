//! An item stack as the mod sends it (`IPacketBuf.writeItemStack`): vanilla's `ItemStack.OPTIONAL_STREAM_CODEC`.
//!
//! ```text
//! VarInt count            0 (or less) is the empty stack and nothing follows
//! VarInt item             the item's id in the item registry (stone is 1; ids are from the game's own table)
//! VarInt added, VarInt removed
//! added   x { VarInt component type id, the component's value }
//! removed x { VarInt component type id }
//! ```
//! Entries keep the order they were written in. A component's value has a codec of its own per type, so only the
//! types listed in [`component`] can be read; any other id is [`DecodeError::UnsupportedComponent`] (the stack
//! cannot be skipped without knowing its length). Ids and codecs are verified against the real game's codecs by
//! tests/oracle_w2.rs (text, and the first components) and tests/oracle_item_components.rs (the rest, with the
//! registry ids in tests/oracle/data_component_ids.tsv); widen the list when a real packet needs another type.
//!
//! Containers (`container`, `bundle_contents`, `charged_projectiles`) hold item stacks themselves, so they are read by
//! the same functions, recursively. Their entries are item *templates* (`ItemStackTemplate`): the same fields, but
//! a count of zero is not "empty", and `container` marks each empty slot with a bool.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::nbt::{read_nbt, write_nbt, Nbt};
use crate::text::{read_text, write_text, Text};

/// Data component type ids (the `DATA_COMPONENT_TYPE` registry order of 26.3, read from the real registry).
pub mod component {
    pub const CUSTOM_DATA: i32 = 0;
    pub const MAX_STACK_SIZE: i32 = 1;
    pub const MAX_DAMAGE: i32 = 2;
    pub const DAMAGE: i32 = 3;
    pub const UNBREAKABLE: i32 = 4;
    pub const CUSTOM_NAME: i32 = 6;
    pub const ITEM_NAME: i32 = 9;
    pub const ITEM_MODEL: i32 = 10;
    pub const LORE: i32 = 11;
    pub const RARITY: i32 = 12;
    pub const ENCHANTMENTS: i32 = 13;
    pub const REPAIR_COST: i32 = 19;
    pub const ATTRIBUTE_MODIFIERS: i32 = 16;
    pub const CUSTOM_MODEL_DATA: i32 = 17;
    pub const TOOLTIP_DISPLAY: i32 = 18;
    pub const STORED_ENCHANTMENTS: i32 = 45;
    pub const DYED_COLOR: i32 = 47;
    pub const MAP_ID: i32 = 48;
    pub const CHARGED_PROJECTILES: i32 = 51;
    pub const BUNDLE_CONTENTS: i32 = 52;
    pub const POTION_CONTENTS: i32 = 53;
    pub const TRIM: i32 = 58;
    pub const INSTRUMENT: i32 = 63;
    pub const OMINOUS_BOTTLE_AMPLIFIER: i32 = 65;
    pub const JUKEBOX_PLAYABLE: i32 = 66;
    pub const FIREWORK_EXPLOSION: i32 = 70;
    pub const FIREWORKS: i32 = 71;
    pub const BANNER_PATTERNS: i32 = 74;
    pub const BASE_COLOR: i32 = 75;
    pub const CONTAINER: i32 = 77;
    pub const WRITTEN_BOOK_CONTENT: i32 = 57;
    pub const PROFILE: i32 = 72;
}

/// A banner pattern as a layer carries it: a holder of the dynamic `banner_pattern` registry, sent as the entry's
/// id plus one (the server's registry order, learnt in the configuration phase and not resolved here), or `0` and the
/// pattern written out.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternRef {
    Registered(i32),
    Inline { asset_id: String, translation_key: String },
}

/// One layer of `banner_patterns`: a pattern and the id of its dye colour.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerLayer {
    pub pattern: PatternRef,
    pub color: i32,
}

/// One entry of `enchantments` or `stored_enchantments`: an enchantment (its registry id) and its level.
#[derive(Debug, Clone, PartialEq)]
pub struct EnchantmentLevel {
    pub enchantment: i32,
    pub level: i32,
}

/// The part of a mob effect after its holder: amplifier, duration, the three flags, and the hidden effect a
/// potion's effect may show in its place when the viewer lacks the effect's icon (none in practice; read as it comes).
#[derive(Debug, Clone, PartialEq)]
pub struct MobEffectDetails {
    pub amplifier: i32,
    pub duration: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
    pub hidden: Option<Box<MobEffectDetails>>,
}

/// A mob effect on a potion: the effect's registry id, then its details.
#[derive(Debug, Clone, PartialEq)]
pub struct MobEffect {
    pub effect: i32,
    pub details: MobEffectDetails,
}

/// `potion_contents`: the potion (a registry id), a colour override, the extra effects, and a custom name.
#[derive(Debug, Clone, PartialEq)]
pub struct PotionContents {
    pub potion: Option<i32>,
    pub custom_color: Option<i32>,
    pub effects: Vec<MobEffect>,
    pub custom_name: Option<String>,
}

/// `firework_explosion` (also each entry of `fireworks`): a shape id, colours, fade colours, a trail and a twinkle.
#[derive(Debug, Clone, PartialEq)]
pub struct FireworkExplosion {
    pub shape: i32,
    pub colors: Vec<i32>,
    pub fade_colors: Vec<i32>,
    pub has_trail: bool,
    pub has_twinkle: bool,
}

/// How an attribute modifier shows on the tooltip: the default line, hidden, or a text of its own.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeDisplay {
    Default,
    Hidden,
    Override(Text),
}

/// One entry of `attribute_modifiers`: an attribute (registry id), a modifier (its identifier, amount, operation),
/// the equipment slot group it applies in, and its tooltip display.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeModifier {
    pub attribute: i32,
    pub id: String,
    pub amount: f64,
    pub operation: i32,
    pub slot_group: i32,
    pub display: AttributeDisplay,
}

/// A text a player wrote, as the server keeps it: the raw version and, when the chat filter changed it, the filtered one
/// (`Filterable`). Both are sent with a presence flag for the filtered one.
#[derive(Debug, Clone, PartialEq)]
pub struct Filterable<T> {
    pub raw: T,
    pub filtered: Option<T>,
}

/// One property of a game profile: the skin textures and other data a profile service sent, with an optional signature.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

/// A profile whose uuid and name are both known (`GameProfile`): the uuid, the name, and its properties in order.
#[derive(Debug, Clone, PartialEq)]
pub struct GameProfile {
    pub id: u128,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

/// A profile the receiver still has to look up (`ResolvableProfile.Partial`): a name, a uuid, or both, and its properties.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialProfile {
    pub name: Option<String>,
    pub id: Option<u128>,
    pub properties: Vec<ProfileProperty>,
}

/// The skin a profile overrides: the textures as identifiers (the body, the cape, the elytra) and the model as the
/// enum's ordinal (`PlayerModelType`, 0 wide and 1 slim).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinPatch {
    pub body: Option<String>,
    pub cape: Option<String>,
    pub elytra: Option<String>,
    pub model: Option<i32>,
}

/// Where a profile's identity comes from: a resolved `GameProfile` (sent as the left side of an `Either`, a flag of
/// true) or a partial one that the receiver resolves (the right side, a flag of false).
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileSource {
    Resolved(GameProfile),
    Partial(PartialProfile),
}

/// `profile` (`ResolvableProfile`): a player's profile, e.g. the owner of a player head.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvableProfile {
    pub source: ProfileSource,
    pub skin: SkinPatch,
}

/// `written_book_content`: the title (filterable), the author, the generation, the pages (filterable text components),
/// and whether those components are already resolved (vanilla's `resolved` flag).
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenBookContent {
    pub title: Filterable<String>,
    pub author: String,
    pub generation: i32,
    pub pages: Vec<Filterable<Text>>,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
    /// `max_stack_size`, `max_damage`, `damage`, `base_color` (a dye colour's id), `repair_cost`, `map_id`,
    /// `ominous_bottle_amplifier`, `rarity` (the enum's ordinal): a VarInt.
    VarInt(i32),
    /// `custom_name`, `item_name`: one text component.
    Text(Text),
    /// `lore`: a VarInt count, then that many text components.
    TextList(Vec<Text>),
    /// `item_model`: an identifier, a VarInt-length string.
    Identifier(String),
    /// `custom_data`: a compound tag.
    Nbt(Nbt),
    /// `banner_patterns`: a VarInt count of layers, each a pattern holder and a dye colour id.
    BannerPatterns(Vec<BannerLayer>),
    /// `tooltip_display`: a bool (hide the whole tooltip), then a VarInt count of component type ids to hide, then the ids.
    TooltipDisplay { hide: bool, hidden: Vec<i32> },
    /// `unbreakable`: no value at all.
    Unit,
    /// `dyed_color`: an int, four bytes big-endian (the colour's RGB).
    Int(i32),
    /// `instrument`, `jukebox_playable`: one holder of a data registry, as its registry id. The wire value is the id
    /// plus one (`0` would be an inline value, which is not read).
    RegistryId(i32),
    /// `enchantments`, `stored_enchantments`: a VarInt count, then per entry a holder (raw registry id) and a VarInt level.
    Enchantments(Vec<EnchantmentLevel>),
    /// `potion_contents`.
    PotionContents(PotionContents),
    /// `firework_explosion`.
    FireworkExplosion(FireworkExplosion),
    /// `fireworks`: a VarInt flight duration, then a VarInt count of explosions.
    Fireworks { flight_duration: i32, explosions: Vec<FireworkExplosion> },
    /// `custom_model_data`: four lists, each a VarInt count then the items: floats, bools, strings, colours (ints).
    CustomModelData { floats: Vec<f32>, flags: Vec<bool>, strings: Vec<String>, colors: Vec<i32> },
    /// `trim`: the armour material and pattern, as registry ids (each sent as the id plus one, like [`ComponentValue::RegistryId`]).
    Trim { material: i32, pattern: i32 },
    /// `attribute_modifiers`: a VarInt count of entries.
    AttributeModifiers(Vec<AttributeModifier>),
    /// `container`: a VarInt count of slots; each is a bool (false is an empty slot) and, if true, an item template.
    Container(Vec<Option<ItemStack>>),
    /// `bundle_contents`, `charged_projectiles`: a VarInt count of item templates.
    Stacks(Vec<ItemStack>),
    /// `profile`: the source (a flag, then a resolved profile or a partial one), then the skin patch.
    Profile(ResolvableProfile),
    /// `written_book_content`.
    WrittenBook(WrittenBookContent),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    pub count: i32,
    /// The item registry id.
    pub item: i32,
    pub added: Vec<(i32, ComponentValue)>,
    pub removed: Vec<i32>,
}

/// A VarInt length or count; a negative one is an error.
fn read_len(r: &mut Reader<'_>) -> Result<usize> {
    usize::try_from(r.read_varint()?).map_err(|_| DecodeError::NegativeLength(-1))
}

/// A holder sent as its registry id plus one (`ByteBufCodecs.holder`): `0` is an inline value, which is not read.
fn read_holder(r: &mut Reader<'_>) -> Result<i32> {
    match r.read_varint()? {
        0 => Err(DecodeError::Unsupported("an inline (not registered) holder in a data component")),
        wire => Ok(wire - 1),
    }
}

fn write_mob_effect_details(w: &mut Writer, details: &MobEffectDetails) {
    w.write_varint(details.amplifier);
    w.write_varint(details.duration);
    w.write_bool(details.ambient);
    w.write_bool(details.visible);
    w.write_bool(details.show_icon);
    match &details.hidden {
        Some(hidden) => {
            w.write_bool(true);
            write_mob_effect_details(w, hidden);
        }
        None => w.write_bool(false),
    }
}

/// One level of the details chain, without its hidden effect.
fn read_mob_effect_level(r: &mut Reader<'_>) -> Result<MobEffectDetails> {
    Ok(MobEffectDetails {
        amplifier: r.read_varint()?,
        duration: r.read_varint()?,
        ambient: r.read_bool()?,
        visible: r.read_bool()?,
        show_icon: r.read_bool()?,
        hidden: None,
    })
}

/// The details of a mob effect, with its hidden effect chain. Each hidden level is a flag and then the same fields;
/// the chain is read in a loop, so a long one cannot exhaust the stack.
fn read_mob_effect_details(r: &mut Reader<'_>) -> Result<MobEffectDetails> {
    let mut levels = vec![read_mob_effect_level(r)?];
    while r.read_bool()? {
        levels.push(read_mob_effect_level(r)?);
    }
    let mut outermost = None;
    for mut level in levels.into_iter().rev() {
        level.hidden = outermost.take().map(Box::new);
        outermost = Some(level);
    }
    outermost.ok_or(DecodeError::Unsupported("a mob effect with no level"))
}

fn write_firework(w: &mut Writer, f: &FireworkExplosion) {
    w.write_varint(f.shape);
    w.write_varint(f.colors.len() as i32);
    for color in &f.colors {
        w.write_i32(*color);
    }
    w.write_varint(f.fade_colors.len() as i32);
    for color in &f.fade_colors {
        w.write_i32(*color);
    }
    w.write_bool(f.has_trail);
    w.write_bool(f.has_twinkle);
}

fn read_firework(r: &mut Reader<'_>) -> Result<FireworkExplosion> {
    let shape = r.read_varint()?;
    let colors = read_ints(r)?;
    let fade_colors = read_ints(r)?;
    Ok(FireworkExplosion { shape, colors, fade_colors, has_trail: r.read_bool()?, has_twinkle: r.read_bool()? })
}

fn read_ints(r: &mut Reader<'_>) -> Result<Vec<i32>> {
    let len = read_len(r)?;
    let mut values = Vec::new();
    for _ in 0..len {
        values.push(r.read_i32()?);
    }
    Ok(values)
}

/// `Filterable.streamCodec`: the raw value, then a flag and the filtered value when present.
fn write_filterable<T>(w: &mut Writer, value: &Filterable<T>, write: impl Fn(&mut Writer, &T) -> Result<()>) -> Result<()> {
    write(w, &value.raw)?;
    w.write_bool(value.filtered.is_some());
    if let Some(filtered) = &value.filtered {
        write(w, filtered)?;
    }
    Ok(())
}

fn read_filterable<T>(r: &mut Reader<'_>, read: impl Fn(&mut Reader<'_>) -> Result<T>) -> Result<Filterable<T>> {
    let raw = read(r)?;
    let filtered = r.read_nullable(&read)?;
    Ok(Filterable { raw, filtered })
}

fn write_profile_property(w: &mut Writer, property: &ProfileProperty) {
    w.write_string(&property.name);
    w.write_string(&property.value);
    w.write_nullable(property.signature.as_ref(), |w, signature| w.write_string(signature));
}

fn read_profile_property(r: &mut Reader<'_>) -> Result<ProfileProperty> {
    let name = r.read_string()?;
    let value = r.read_string()?;
    let signature = r.read_nullable(|r| r.read_string())?;
    Ok(ProfileProperty { name, value, signature })
}

/// A property map: a VarInt count, then the properties in the order the map holds them.
fn write_profile_properties(w: &mut Writer, properties: &[ProfileProperty]) {
    w.write_varint(properties.len() as i32);
    for property in properties {
        write_profile_property(w, property);
    }
}

fn read_profile_properties(r: &mut Reader<'_>) -> Result<Vec<ProfileProperty>> {
    let len = read_len(r)?;
    let mut properties = Vec::new();
    for _ in 0..len {
        properties.push(read_profile_property(r)?);
    }
    Ok(properties)
}

/// `ResolvableProfile`'s stream codec: the source as an `Either` (a flag: true is the resolved profile), then the skin
/// patch (four optionals: body, cape, elytra as identifiers, and the model).
fn write_profile(w: &mut Writer, profile: &ResolvableProfile) {
    match &profile.source {
        ProfileSource::Resolved(game) => {
            w.write_bool(true);
            w.write_uuid(game.id);
            w.write_string(&game.name);
            write_profile_properties(w, &game.properties);
        }
        ProfileSource::Partial(partial) => {
            w.write_bool(false);
            w.write_nullable(partial.name.as_ref(), |w, name| w.write_string(name));
            w.write_nullable(partial.id.as_ref(), |w, id| w.write_uuid(*id));
            write_profile_properties(w, &partial.properties);
        }
    }
    let skin = &profile.skin;
    w.write_nullable(skin.body.as_ref(), |w, id| w.write_string(id));
    w.write_nullable(skin.cape.as_ref(), |w, id| w.write_string(id));
    w.write_nullable(skin.elytra.as_ref(), |w, id| w.write_string(id));
    w.write_nullable(skin.model.as_ref(), |w, model| w.write_varint(*model));
}

fn read_profile(r: &mut Reader<'_>) -> Result<ResolvableProfile> {
    let source = if r.read_bool()? {
        let id = r.read_uuid()?;
        let name = r.read_string()?;
        ProfileSource::Resolved(GameProfile { id, name, properties: read_profile_properties(r)? })
    } else {
        let name = r.read_nullable(|r| r.read_string())?;
        let id = r.read_nullable(|r| r.read_uuid())?;
        ProfileSource::Partial(PartialProfile { name, id, properties: read_profile_properties(r)? })
    };
    let body = r.read_nullable(|r| r.read_string())?;
    let cape = r.read_nullable(|r| r.read_string())?;
    let elytra = r.read_nullable(|r| r.read_string())?;
    let model = r.read_nullable(|r| r.read_varint())?;
    Ok(ResolvableProfile { source, skin: SkinPatch { body, cape, elytra, model } })
}

/// `WrittenBookContent`'s stream codec: the title (filterable), the author, the generation, the pages (filterable
/// components, a VarInt count), then the resolved flag.
fn write_book(w: &mut Writer, book: &WrittenBookContent) -> Result<()> {
    write_filterable(w, &book.title, |w, title| {
        w.write_string(title);
        Ok(())
    })?;
    w.write_string(&book.author);
    w.write_varint(book.generation);
    w.write_varint(book.pages.len() as i32);
    for page in &book.pages {
        write_filterable(w, page, write_text)?;
    }
    w.write_bool(book.resolved);
    Ok(())
}

fn read_book(r: &mut Reader<'_>) -> Result<WrittenBookContent> {
    let title = read_filterable(r, |r| r.read_string())?;
    let author = r.read_string()?;
    let generation = r.read_varint()?;
    let len = read_len(r)?;
    let mut pages = Vec::new();
    for _ in 0..len {
        pages.push(read_filterable(r, read_text)?);
    }
    let resolved = r.read_bool()?;
    Ok(WrittenBookContent { title, author, generation, pages, resolved })
}

/// `ItemStackTemplate`: the item, a VarInt count (any value), then the component patch.
fn write_template(w: &mut Writer, stack: &ItemStack) -> Result<()> {
    w.write_varint(stack.item);
    w.write_varint(stack.count);
    write_patch(w, stack)
}

fn read_template(r: &mut Reader<'_>) -> Result<ItemStack> {
    let item = r.read_varint()?;
    let count = r.read_varint()?;
    read_patch(r, count, item)
}

/// The component patch of a stack or template: the added entries, then the removed ids.
fn write_patch(w: &mut Writer, stack: &ItemStack) -> Result<()> {
    w.write_varint(stack.added.len() as i32);
    w.write_varint(stack.removed.len() as i32);
    for (id, value) in &stack.added {
        w.write_varint(*id);
        write_value(w, *id, value)?;
    }
    for id in &stack.removed {
        w.write_varint(*id);
    }
    Ok(())
}

fn read_patch(r: &mut Reader<'_>, count: i32, item: i32) -> Result<ItemStack> {
    let added = read_len(r)?;
    let removed = read_len(r)?;
    let mut added_values = Vec::new();
    for _ in 0..added {
        let id = r.read_varint()?;
        added_values.push((id, read_value(r, id)?));
    }
    let mut removed_ids = Vec::new();
    for _ in 0..removed {
        removed_ids.push(r.read_varint()?);
    }
    Ok(ItemStack { count, item, added: added_values, removed: removed_ids })
}

fn write_value(w: &mut Writer, id: i32, value: &ComponentValue) -> Result<()> {
    match (id, value) {
        (
            component::MAX_STACK_SIZE
            | component::MAX_DAMAGE
            | component::DAMAGE
            | component::BASE_COLOR
            | component::REPAIR_COST
            | component::RARITY
            | component::MAP_ID
            | component::OMINOUS_BOTTLE_AMPLIFIER,
            ComponentValue::VarInt(v),
        ) => w.write_varint(*v),
        (component::CUSTOM_NAME | component::ITEM_NAME, ComponentValue::Text(t)) => write_text(w, t)?,
        (component::LORE, ComponentValue::TextList(lines)) => {
            w.write_varint(lines.len() as i32);
            for line in lines {
                write_text(w, line)?;
            }
        }
        (component::ITEM_MODEL, ComponentValue::Identifier(s)) => w.write_string(s),
        (component::CUSTOM_DATA, ComponentValue::Nbt(n)) => write_nbt(w, n)?,
        (component::BANNER_PATTERNS, ComponentValue::BannerPatterns(layers)) => {
            w.write_varint(layers.len() as i32);
            for layer in layers {
                match &layer.pattern {
                    PatternRef::Registered(id) => w.write_varint(*id),
                    PatternRef::Inline { asset_id, translation_key } => {
                        w.write_varint(0);
                        w.write_string(asset_id);
                        w.write_string(translation_key);
                    }
                }
                w.write_varint(layer.color);
            }
        }
        (component::TOOLTIP_DISPLAY, ComponentValue::TooltipDisplay { hide, hidden }) => {
            w.write_bool(*hide);
            w.write_varint(hidden.len() as i32);
            for id in hidden {
                w.write_varint(*id);
            }
        }
        (component::UNBREAKABLE, ComponentValue::Unit) => {}
        (component::DYED_COLOR, ComponentValue::Int(v)) => w.write_i32(*v),
        (component::INSTRUMENT | component::JUKEBOX_PLAYABLE, ComponentValue::RegistryId(v)) => w.write_varint(v + 1),
        (component::ENCHANTMENTS | component::STORED_ENCHANTMENTS, ComponentValue::Enchantments(entries)) => {
            w.write_varint(entries.len() as i32);
            for entry in entries {
                w.write_varint(entry.enchantment);
                w.write_varint(entry.level);
            }
        }
        (component::POTION_CONTENTS, ComponentValue::PotionContents(p)) => {
            w.write_bool(p.potion.is_some());
            if let Some(potion) = p.potion {
                w.write_varint(potion);
            }
            w.write_bool(p.custom_color.is_some());
            if let Some(color) = p.custom_color {
                w.write_i32(color);
            }
            w.write_varint(p.effects.len() as i32);
            for effect in &p.effects {
                w.write_varint(effect.effect);
                write_mob_effect_details(w, &effect.details);
            }
            w.write_bool(p.custom_name.is_some());
            if let Some(name) = &p.custom_name {
                w.write_string(name);
            }
        }
        (component::FIREWORK_EXPLOSION, ComponentValue::FireworkExplosion(f)) => write_firework(w, f),
        (component::FIREWORKS, ComponentValue::Fireworks { flight_duration, explosions }) => {
            w.write_varint(*flight_duration);
            w.write_varint(explosions.len() as i32);
            for explosion in explosions {
                write_firework(w, explosion);
            }
        }
        (component::CUSTOM_MODEL_DATA, ComponentValue::CustomModelData { floats, flags, strings, colors }) => {
            w.write_varint(floats.len() as i32);
            for f in floats {
                w.write_f32(*f);
            }
            w.write_varint(flags.len() as i32);
            for flag in flags {
                w.write_bool(*flag);
            }
            w.write_varint(strings.len() as i32);
            for s in strings {
                w.write_string(s);
            }
            w.write_varint(colors.len() as i32);
            for color in colors {
                w.write_i32(*color);
            }
        }
        (component::TRIM, ComponentValue::Trim { material, pattern }) => {
            w.write_varint(material + 1);
            w.write_varint(pattern + 1);
        }
        (component::ATTRIBUTE_MODIFIERS, ComponentValue::AttributeModifiers(entries)) => {
            w.write_varint(entries.len() as i32);
            for entry in entries {
                w.write_varint(entry.attribute);
                w.write_string(&entry.id);
                w.write_f64(entry.amount);
                w.write_varint(entry.operation);
                w.write_varint(entry.slot_group);
                match &entry.display {
                    AttributeDisplay::Default => w.write_varint(0),
                    AttributeDisplay::Hidden => w.write_varint(1),
                    AttributeDisplay::Override(text) => {
                        w.write_varint(2);
                        write_text(w, text)?;
                    }
                }
            }
        }
        (component::CONTAINER, ComponentValue::Container(slots)) => {
            w.write_varint(slots.len() as i32);
            for slot in slots {
                match slot {
                    Some(stack) => {
                        w.write_bool(true);
                        write_template(w, stack)?;
                    }
                    None => w.write_bool(false),
                }
            }
        }
        (component::BUNDLE_CONTENTS | component::CHARGED_PROJECTILES, ComponentValue::Stacks(stacks)) => {
            w.write_varint(stacks.len() as i32);
            for stack in stacks {
                write_template(w, stack)?;
            }
        }
        (component::PROFILE, ComponentValue::Profile(profile)) => write_profile(w, profile),
        (component::WRITTEN_BOOK_CONTENT, ComponentValue::WrittenBook(book)) => write_book(w, book)?,
        _ => return Err(DecodeError::UnsupportedComponent(id)),
    }
    Ok(())
}

fn read_value(r: &mut Reader<'_>, id: i32) -> Result<ComponentValue> {
    Ok(match id {
        component::MAX_STACK_SIZE
        | component::MAX_DAMAGE
        | component::DAMAGE
        | component::BASE_COLOR
        | component::REPAIR_COST
        | component::RARITY
        | component::MAP_ID
        | component::OMINOUS_BOTTLE_AMPLIFIER => ComponentValue::VarInt(r.read_varint()?),
        component::CUSTOM_NAME | component::ITEM_NAME => ComponentValue::Text(read_text(r)?),
        component::LORE => {
            let len = read_len(r)?;
            let mut lines = Vec::new();
            for _ in 0..len {
                lines.push(read_text(r)?);
            }
            ComponentValue::TextList(lines)
        }
        component::ITEM_MODEL => ComponentValue::Identifier(r.read_string()?),
        component::CUSTOM_DATA => ComponentValue::Nbt(read_nbt(r)?),
        component::BANNER_PATTERNS => {
            let len = read_len(r)?;
            let mut layers = Vec::new();
            for _ in 0..len {
                let holder = r.read_varint()?;
                let pattern = if holder == 0 {
                    PatternRef::Inline { asset_id: r.read_string()?, translation_key: r.read_string()? }
                } else {
                    PatternRef::Registered(holder)
                };
                layers.push(BannerLayer { pattern, color: r.read_varint()? });
            }
            ComponentValue::BannerPatterns(layers)
        }
        component::TOOLTIP_DISPLAY => {
            let hide = r.read_bool()?;
            let len = read_len(r)?;
            let mut hidden = Vec::new();
            for _ in 0..len {
                hidden.push(r.read_varint()?);
            }
            ComponentValue::TooltipDisplay { hide, hidden }
        }
        component::UNBREAKABLE => ComponentValue::Unit,
        component::DYED_COLOR => ComponentValue::Int(r.read_i32()?),
        component::INSTRUMENT | component::JUKEBOX_PLAYABLE => ComponentValue::RegistryId(read_holder(r)?),
        component::ENCHANTMENTS | component::STORED_ENCHANTMENTS => {
            let len = read_len(r)?;
            let mut entries = Vec::new();
            for _ in 0..len {
                entries.push(EnchantmentLevel { enchantment: r.read_varint()?, level: r.read_varint()? });
            }
            ComponentValue::Enchantments(entries)
        }
        component::POTION_CONTENTS => {
            let potion = if r.read_bool()? { Some(r.read_varint()?) } else { None };
            let custom_color = if r.read_bool()? { Some(r.read_i32()?) } else { None };
            let len = read_len(r)?;
            let mut effects = Vec::new();
            for _ in 0..len {
                let effect = r.read_varint()?;
                effects.push(MobEffect { effect, details: read_mob_effect_details(r)? });
            }
            let custom_name = if r.read_bool()? { Some(r.read_string()?) } else { None };
            ComponentValue::PotionContents(PotionContents { potion, custom_color, effects, custom_name })
        }
        component::FIREWORK_EXPLOSION => ComponentValue::FireworkExplosion(read_firework(r)?),
        component::FIREWORKS => {
            let flight_duration = r.read_varint()?;
            let len = read_len(r)?;
            let mut explosions = Vec::new();
            for _ in 0..len {
                explosions.push(read_firework(r)?);
            }
            ComponentValue::Fireworks { flight_duration, explosions }
        }
        component::CUSTOM_MODEL_DATA => {
            let mut floats = Vec::new();
            for _ in 0..read_len(r)? {
                floats.push(r.read_f32()?);
            }
            let mut flags = Vec::new();
            for _ in 0..read_len(r)? {
                flags.push(r.read_bool()?);
            }
            let mut strings = Vec::new();
            for _ in 0..read_len(r)? {
                strings.push(r.read_string()?);
            }
            let colors = read_ints(r)?;
            ComponentValue::CustomModelData { floats, flags, strings, colors }
        }
        component::TRIM => ComponentValue::Trim { material: read_holder(r)?, pattern: read_holder(r)? },
        component::ATTRIBUTE_MODIFIERS => {
            let len = read_len(r)?;
            let mut entries = Vec::new();
            for _ in 0..len {
                let attribute = r.read_varint()?;
                let id = r.read_string()?;
                let amount = r.read_f64()?;
                let operation = r.read_varint()?;
                let slot_group = r.read_varint()?;
                let display = match r.read_varint()? {
                    0 => AttributeDisplay::Default,
                    1 => AttributeDisplay::Hidden,
                    2 => AttributeDisplay::Override(read_text(r)?),
                    _ => return Err(DecodeError::Unsupported("an attribute modifier's display type")),
                };
                entries.push(AttributeModifier { attribute, id, amount, operation, slot_group, display });
            }
            ComponentValue::AttributeModifiers(entries)
        }
        component::CONTAINER => {
            let len = read_len(r)?;
            let mut slots = Vec::new();
            for _ in 0..len {
                slots.push(if r.read_bool()? { Some(read_template(r)?) } else { None });
            }
            ComponentValue::Container(slots)
        }
        component::BUNDLE_CONTENTS | component::CHARGED_PROJECTILES => {
            let len = read_len(r)?;
            let mut stacks = Vec::new();
            for _ in 0..len {
                stacks.push(read_template(r)?);
            }
            ComponentValue::Stacks(stacks)
        }
        component::PROFILE => ComponentValue::Profile(read_profile(r)?),
        component::WRITTEN_BOOK_CONTENT => ComponentValue::WrittenBook(read_book(r)?),
        other => return Err(DecodeError::UnsupportedComponent(other)),
    })
}

/// `OPTIONAL_STREAM_CODEC.encode`: `None` is the empty stack.
pub fn write_item_stack(w: &mut Writer, stack: Option<&ItemStack>) -> Result<()> {
    let Some(stack) = stack else {
        w.write_varint(0);
        return Ok(());
    };
    w.write_varint(stack.count);
    w.write_varint(stack.item);
    write_patch(w, stack)
}

/// `OPTIONAL_STREAM_CODEC.decode`: a count of zero or less is the empty stack (`None`).
pub fn read_item_stack(r: &mut Reader<'_>) -> Result<Option<ItemStack>> {
    let count = r.read_varint()?;
    if count <= 0 {
        return Ok(None);
    }
    let item = r.read_varint()?;
    read_patch(r, count, item).map(Some)
}
