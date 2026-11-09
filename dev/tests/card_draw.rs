//! The card layout against the numbers in the original renderer (ClientCardBufferRenderer.kt, CardTileRenderer.kt).
use horizon_bingo::card_draw::*;
use horizon_bingo::card_tile::{CardTile, TierLabel};
use horizon_bingo::card_tile_action::CardTileAction;
use horizon_bingo::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use horizon_bingo::item::ItemStack;
use horizon_bingo::state::{empty_tile, ClientCard};
use horizon_bingo::text::Text;
use horizon_bingo::text_plain::{flatten, format_template};

struct Names;
impl ItemNames for Names {
    fn item_name(&self, id: i32) -> Option<String> {
        (id != 999).then(|| format!("item_{id}"))
    }
}

fn stack(id: i32) -> ItemStack {
    ItemStack { count: 1, item: id, added: vec![], removed: vec![] }
}

fn tile_with(f: impl FnOnce(&mut CardTile)) -> CardTile {
    let mut t = empty_tile();
    t.image = CardTileImage { item: Some(stack(7)), ..CardTileImage::default() };
    t.action = CardTileAction::None;
    f(&mut t);
    t
}

/// A card with `tile` at index 6 (x = 1, y = 1: origin (32, 42)) and nothing else.
fn card(tile: CardTile) -> ClientCard {
    let mut tiles = vec![empty_tile(); 25];
    tiles[6] = tile;
    let mut c = ClientCard::new_for_tests();
    c.tiles = tiles;
    c.tile_updated_at = vec![None; 25];
    c
}

fn draw(card: &ClientCard, now: u64, outlines: bool) -> Vec<Draw> {
    let colors = TeamColors::default();
    let outline = |t: &str| if t == "a" { 0xffff5555 } else { 0xff5555ff };
    draw_card(&CardView { card, colors: &colors, now_ms: now, show_outlines: outlines, team_outline: &outline, items: &Names, images: None, team_name: None , is_gui: false })
}

fn fills(d: &[Draw]) -> Vec<(i32, i32, i32, i32, u32)> {
    d.iter().filter_map(|c| if let Draw::Fill { x, y, w, h, argb } = c { Some((*x, *y, *w, *h, *argb)) } else { None }).collect()
}

fn items(d: &[Draw]) -> Vec<(String, f32, f32, f32)> {
    d.iter().filter_map(|c| if let Draw::Item { name, x, y, size } = c { Some((name.clone(), *x, *y, *size)) } else { None }).collect()
}

#[test]
fn the_background_comes_first_and_the_team_name_is_black_at_12_10() {
    let c = card(tile_with(|_| {}));
    let colors = TeamColors { card_texture: "bingo/card_red".into(), ..TeamColors::default() };
    let d = draw_card(&CardView { card: &c, colors: &colors, now_ms: 0, show_outlines: false, team_outline: &|_| 0, items: &Names, images: None, team_name: Some("Red Team".into()), is_gui: false });
    assert_eq!(d[0], Draw::Sprite { name: "bingo/card_red".into(), x: 0.0, y: 0.0, w: 122.0, h: 132.0 });
    assert_eq!(d[1], Draw::Text { x: 12, y: 10, text: "Red Team".into(), argb: 0xff000000 });
}

#[test]
fn a_tile_item_is_16_pixels_one_pixel_inside_the_tile_at_12_plus_20x_22_plus_20y() {
    let d = draw(&card(tile_with(|_| {})), 0, false);
    // tile index 6: x = 1, y = 1 -> origin (12 + 20, 22 + 20) = (32, 42); the item at (33, 43)
    assert_eq!(items(&d), [("item_7".into(), 33.0, 43.0, 16.0)]);
    // An item the table does not know is not drawn.
    let unknown = draw(&card(tile_with(|t| t.image.item = Some(stack(999)))), 0, false);
    assert!(items(&unknown).is_empty());
}

#[test]
fn progress_fills_the_tile_from_the_bottom() {
    // fill(0, (18 * (1 - progress)).toInt(), 18, 18): progress 0.5 -> from y = 9 to 18
    let half = draw(&card(tile_with(|t| t.progress = 0.5)), 0, false);
    assert_eq!(fills(&half), [(32, 42 + 9, 18, 9, TILE_PROGRESS)]);
    // exactly 1/18 is the smallest that draws: y = (18 * 17/18).toInt() = 17, one pixel high
    let smallest = draw(&card(tile_with(|t| t.progress = 1.0 / 18.0)), 0, false);
    assert_eq!(fills(&smallest), [(32, 42 + 17, 18, 1, TILE_PROGRESS)]);
    let below = draw(&card(tile_with(|t| t.progress = 1.0 / 18.0 - 0.001)), 0, false);
    assert!(fills(&below).is_empty());
    // a fractional height is cut, not rounded: 18 * (1 - 0.3) = 12.6 -> 12, so six rows are filled
    let third = draw(&card(tile_with(|t| t.progress = 0.3)), 0, false);
    assert_eq!(fills(&third), [(32, 42 + 12, 18, 6, TILE_PROGRESS)]);
    // a full tile fills all 18 rows
    let full = draw(&card(tile_with(|t| t.progress = 1.0)), 0, false);
    assert_eq!(fills(&full), [(32, 42, 18, 18, TILE_PROGRESS)]);
}

#[test]
fn an_achieved_tile_is_green_under_its_item_and_dimmed_over_it() {
    let d = draw(&card(tile_with(|t| t.image.is_achieved = true)), 0, false);
    // 0xff / 4 = 63 = 0x3f of alpha, the colour unchanged
    assert_eq!(fills(&d), [(32, 42, 18, 18, TILE_ACHIEVED), (32, 42, 18, 18, 0x3f55ff55)]);
    let order: Vec<&str> = d.iter().map(|c| match c { Draw::Fill { .. } => "fill", Draw::Item { .. } => "item", _ => "other" }).collect();
    assert_eq!(&order[1..], ["fill", "item", "fill"], "the green goes under the item, the overlay over it");
}

#[test]
fn a_flashing_tile_blinks_every_500_ms_and_takes_the_place_of_green() {
    let achieved_and_flashing = card(tile_with(|t| {
        t.image.is_achieved = true;
        t.is_flashing = true;
    }));
    // (now / 500) % 2 == 0: on at 0 and 1000, off at 500
    let on = draw(&achieved_and_flashing, 0, false);
    assert_eq!(fills(&on), [(32, 42, 18, 18, TILE_FLASHING)], "flashing wins over achieved and has no overlay");
    let off = draw(&achieved_and_flashing, 500, false);
    assert_eq!(fills(&off), [(32, 42, 18, 18, TILE_ACHIEVED), (32, 42, 18, 18, 0x3f55ff55)]);
    assert_eq!(fills(&draw(&achieved_and_flashing, 1_000, false)), fills(&on));
}

#[test]
fn a_tile_that_just_changed_blinks_out_for_4_seconds() {
    let mut c = card(tile_with(|_| {}));
    c.tile_updated_at[6] = Some(1_000);
    let drawn = |now| !items(&draw(&c, now, false)).is_empty();
    assert!(!drawn(1_000), "0 ms since: (0 / 500) % 2 == 0, gone");
    assert!(!drawn(1_499));
    assert!(drawn(1_500), "500 ms since: back");
    assert!(!drawn(2_000), "1000 ms since: gone again");
    assert!(!drawn(4_000), "3000 ms since: an even interval inside the 4 seconds, still gone");
    assert!(drawn(4_500), "3500 ms since: odd, drawn");
    assert!(drawn(4_999), "3999 ms since: odd interval, drawn");
    assert!(drawn(5_000), "4000 ms since: the blinking is over");
    assert!(drawn(5_500));
}

#[test]
fn a_hidden_tile_shows_the_hidden_sprite_of_its_tier() {
    let tier = |t: Option<TierLabel>| {
        let d = draw(&card(tile_with(|x| {
            x.is_hidden = true;
            x.item_tier = t;
        })), 0, false);
        d.into_iter().find_map(|c| if let Draw::Sprite { name, x, y, w, h } = c { (name != "bingo/card_preview").then_some((name, x, y, w, h)) } else { None })
    };
    assert_eq!(tier(Some(TierLabel::C)), Some(("bingo/image_hidden_c".into(), 33.0, 43.0, 16.0, 16.0)));
    assert_eq!(tier(Some(TierLabel::S)).unwrap().0, "bingo/image_hidden_s");
    assert_eq!(tier(None).unwrap().0, "bingo/image_hidden");
}

#[test]
fn a_one_of_tile_stacks_up_to_three_12_pixel_items_2_pixels_apart_with_the_first_on_top() {
    let list = vec![
        CardTileImage { item: Some(stack(1)), ..Default::default() },
        CardTileImage { item: Some(stack(2)), ..Default::default() },
        CardTileImage { item: Some(stack(3)), ..Default::default() },
        CardTileImage { item: Some(stack(4)), ..Default::default() },
    ];
    let c = card(tile_with(|t| {
        t.image.item_style = ItemStyle::OneOf;
        t.image_list = list;
    }));
    // (scale 12/16, step 4 / scale / 2 in scaled units = 2 px); painted last first so item 1 ends up on top. The fourth is not drawn.
    assert_eq!(items(&draw(&c, 0, false)), [
        ("item_3".into(), 37.0, 47.0, 12.0),
        ("item_2".into(), 35.0, 45.0, 12.0),
        ("item_1".into(), 33.0, 43.0, 12.0),
    ]);
}

#[test]
fn an_all_of_tile_is_a_two_by_two_grid_of_9_pixel_items() {
    let list: Vec<CardTileImage> = (1..=4).map(|i| CardTileImage { item: Some(stack(i)), ..Default::default() }).collect();
    let c = card(tile_with(|t| {
        t.image.item_style = ItemStyle::AllOf;
        t.image_list = list;
    }));
    // scale 9/16, cells 12 scaled units apart = 6.75 px
    assert_eq!(items(&draw(&c, 0, false)), [
        ("item_1".into(), 33.0, 43.0, 9.0),
        ("item_2".into(), 39.75, 43.0, 9.0),
        ("item_3".into(), 33.0, 49.75, 9.0),
        ("item_4".into(), 39.75, 49.75, 9.0),
    ]);
    // One image in the list is not a multi-item tile: the main image is drawn normally.
    let single = card(tile_with(|t| {
        t.image.item_style = ItemStyle::AllOf;
        t.image_list = vec![CardTileImage { item: Some(stack(9)), ..Default::default() }];
    }));
    assert_eq!(items(&draw(&single, 0, false)), [("item_7".into(), 33.0, 43.0, 16.0)]);
}

#[test]
fn decorations_and_the_lock_are_20_pixel_frames_one_pixel_outside_the_tile() {
    let sprites = |c: &ClientCard| -> Vec<(String, f32, f32, f32, f32)> {
        draw(c, 0, false).into_iter().skip(1).filter_map(|d| if let Draw::Sprite { name, x, y, w, h } = d { Some((name, x, y, w, h)) } else { None }).collect()
    };
    let adv = card(tile_with(|t| t.image.decoration = Decoration::Advancement));
    assert_eq!(sprites(&adv), [("bingo/image_frame_advancement".into(), 31.0, 41.0, 20.0, 20.0)]);
    let forbidden = card(tile_with(|t| t.image.decoration = Decoration::Forbidden));
    assert_eq!(sprites(&forbidden)[0].0, "bingo/image_frame_forbidden");
    let free = card(tile_with(|t| t.image.decoration = Decoration::FreeSpace));
    assert_eq!(sprites(&free)[0].0, "bingo/image_frame_free_space");
    assert!(sprites(&card(tile_with(|t| t.image.decoration = Decoration::MultiItem))).is_empty());
    let custom = card(tile_with(|t| t.image.decoration_texture = Some("minecraft:bingo/my_frame".into())));
    assert_eq!(sprites(&custom)[0].0, "bingo/my_frame");
    let locked = card(tile_with(|t| t.is_locked = true));
    assert_eq!(sprites(&locked), [("bingo/image_frame_locked".into(), 31.0, 41.0, 20.0, 20.0)]);
}

#[test]
fn one_team_outlines_the_whole_tile_two_teams_split_it_top_left_and_bottom_right() {
    let one = draw(&card(tile_with(|t| t.team_keys = vec!["a".into()])), 0, true);
    let lines: Vec<_> = fills(&one);
    // origin (32, 42). color = 0xffff5555, dark = each channel * 191 / 255 = 0xffbf3f3f
    let (c, k) = (0xffff5555u32, 0xffbf3f3fu32);
    assert_eq!(lines, [
        (31, 41, 20, 1, c), (31, 41, 1, 20, c),      // top and left, outer
        (31, 60, 20, 1, k), (50, 41, 1, 20, k),      // bottom and right, outer (dark)
        (32, 42, 18, 1, k), (32, 42, 1, 18, k),      // top and left, inner (dark)
        (32, 59, 18, 1, c), (49, 42, 1, 18, c),      // bottom and right, inner
    ]);
    let two = draw(&card(tile_with(|t| t.team_keys = vec!["a".into(), "b".into()])), 0, true);
    let l = fills(&two);
    assert_eq!(l.len(), 8, "four lines for each team, not eight");
    // the first team gets the top and left outer lines in its colour, the second the bottom and right ones, dark
    assert_eq!(l[0], (31, 41, 20, 1, c));
    assert_eq!(l[2], (32, 42, 18, 1, k));
    assert_eq!(l[4], (31, 60, 20, 1, 0xff3f3fbf), "the second team's dark blue: 0x55 * 191 / 255 = 0x3f, 0xff * 191 / 255 = 0xbf");
    assert_eq!(l[6], (32, 59, 18, 1, 0xff5555ff));
    // outlines off: none of it
    assert!(fills(&draw(&card(tile_with(|t| t.team_keys = vec!["a".into()])), 0, false)).is_empty());
}

#[test]
fn a_name_is_cut_to_70_pixels_without_an_ellipsis() {
    let six = |s: &str| 6 * s.chars().count() as i32;
    assert_eq!(truncate_to_width("Short", 70, &six), "Short");
    assert_eq!(truncate_to_width("abcdefghijklmnop", 70, &six), "abcdefghijk", "11 characters are 66 px, 12 are 72");
    assert_eq!(truncate_to_width("", 70, &six), "");
}

#[test]
fn a_translation_falls_back_to_the_components_own_fallback_then_the_key() {
    use horizon_bingo::nbt::Nbt;
    // The team name the real server sent: translate bingo.card.team_card_title (fallback "%s Team") with the argument
    // translate bingo.team.red (fallback "Red").
    let arg = Nbt::compound(vec![("translate", Nbt::string("bingo.team.red")), ("fallback", Nbt::string("Red"))]);
    let title = Text(Nbt::compound(vec![
        ("translate", Nbt::string("bingo.card.team_card_title")),
        ("fallback", Nbt::string("%s Team")),
        ("with", Nbt::list(vec![arg])),
    ]));
    let none = |_: &str| None;
    assert_eq!(flatten(&title, &none), "Red Team");
    let some = |k: &str| (k == "bingo.team.red").then(|| "Czerwona".to_string());
    assert_eq!(flatten(&title, &some), "Czerwona Team");
    let both = |k: &str| match k {
        "bingo.team.red" => Some("Czerwona".to_string()),
        "bingo.card.team_card_title" => Some("Drużyna %s".to_string()),
        _ => None,
    };
    assert_eq!(flatten(&title, &both), "Drużyna Czerwona");
    // no fallback and no entry: the key itself, as Minecraft shows it
    assert_eq!(flatten(&Text(Nbt::compound(vec![("translate", Nbt::string("x.y"))])), &none), "x.y");
    // siblings are appended
    let with_extra = Text(Nbt::compound(vec![("text", Nbt::string("a")), ("extra", Nbt::list(vec![Nbt::string("b"), Nbt::string("c")]))]));
    assert_eq!(flatten(&with_extra, &none), "abc");
    assert_eq!(flatten(&Text::plain("zażółć"), &none), "zażółć");
}

#[test]
fn format_templates_follow_java() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(format_template("%s and %s", &a(&["x", "y"])), "x and y");
    assert_eq!(format_template("%2$s %1$s", &a(&["x", "y"])), "y x");
    assert_eq!(format_template("100%%", &a(&[])), "100%");
    assert_eq!(format_template("%s", &a(&[])), "", "a missing argument is empty");
    assert_eq!(format_template("50% off", &a(&[])), "50% off", "a lone percent stays");
}

#[test]
fn the_real_servers_card_draws_25_tiles_a_background_and_the_team_name() {
    let cap = |s: &str| {
        let s = s.trim();
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>()
    };
    let mut state = horizon_bingo::state::HudState::new();
    state.on_packet("yet-another-minecraft-bingo:display_cards", &cap(include_str!("captures/display_cards.hex"))).unwrap();
    state.on_packet("yet-another-minecraft-bingo:update_card_tiles", &cap(include_str!("captures/update_card_tiles.hex"))).unwrap();
    let (_, card) = state.selected_card().unwrap();
    let name = flatten(card.display.team_name.as_ref().unwrap(), &|_: &str| None);
    assert_eq!(name, "Red Team");
    let colors = TeamColors::default();
    let d = draw_card(&CardView { card, colors: &colors, now_ms: 10_000, show_outlines: false, team_outline: &|_| 0, items: &Names, images: None, team_name: Some(name) , is_gui: false });
    assert!(matches!(&d[0], Draw::Sprite { w, h, .. } if *w == 122.0 && *h == 132.0));
    // 25 tiles, every one with an item the table knows: 23 plain item tiles, 2 multi-item tiles with several icons.
    let n_items = items(&d).len();
    assert!(n_items >= 25, "{n_items} item icons");
    // every item sits inside the card
    for (_, x, y, size) in items(&d) {
        assert!(x >= 12.0 && y >= 22.0 && x + size <= 112.0 && y + size <= 122.0, "({x}, {y}) {size}");
    }
}

#[test]
fn a_tiles_picture_wins_over_its_item_when_it_exists_and_the_item_is_the_fallback() {
    let c = card(tile_with(|t| t.image.texture = Some("custom".into())));
    let colors = TeamColors::default();
    let known = |n: &str| n == "img/custom";
    let none = |_: &str| false;
    let drawn = |images: Option<&dyn Fn(&str) -> bool>| {
        draw_card(&CardView { card: &c, colors: &colors, now_ms: 0, show_outlines: false, team_outline: &|_| 0, items: &Names, images, team_name: None , is_gui: false })
    };
    let with = drawn(Some(&known));
    assert!(items(&with).is_empty(), "the item is not drawn under its picture");
    assert!(with.contains(&Draw::Sprite { name: "img/custom".into(), x: 33.0, y: 43.0, w: 16.0, h: 16.0 }));
    // the picture is not there (yet): the item shows
    assert_eq!(items(&drawn(Some(&none))).len(), 1);
    assert_eq!(items(&drawn(None)).len(), 1);
}

fn counts(d: &[Draw]) -> Vec<(String, i32, i32)> {
    d.iter().filter_map(|c| if let Draw::Count { text, right, y } = c { Some((text.clone(), *right, *y)) } else { None }).collect()
}

#[test]
fn a_stack_of_more_than_one_shows_its_count_at_the_icons_bottom_right() {
    let with = |n: i32| tile_with(|t| t.image.item = Some(ItemStack { count: n, ..stack(7) }));
    // the icon is at (33, 43); itemDecorations puts the right edge of the number at x + 17 and its top at y + 9
    assert_eq!(counts(&draw(&card(with(8)), 0, false)), [("8".into(), 50, 52)]);
    assert_eq!(counts(&draw(&card(with(22)), 0, false)), [("22".into(), 50, 52)]);
    assert!(counts(&draw(&card(with(1)), 0, false)).is_empty());
    // the number comes after its icon, so it is drawn over it
    let d = draw(&card(with(8)), 0, false);
    let icon = d.iter().position(|c| matches!(c, Draw::Item { .. })).unwrap();
    let count = d.iter().position(|c| matches!(c, Draw::Count { .. })).unwrap();
    assert!(count > icon);
}

#[test]
fn the_scaled_icons_of_a_multi_item_tile_show_no_count() {
    let mut t = tile_with(|t| t.image.item_style = ItemStyle::AllOf);
    t.image_list = vec![
        CardTileImage { item: Some(ItemStack { count: 8, ..stack(7) }), ..CardTileImage::default() },
        CardTileImage { item: Some(ItemStack { count: 8, ..stack(8) }), ..CardTileImage::default() },
    ];
    assert!(counts(&draw(&card(t), 0, false)).is_empty());
}

#[test]
fn an_achieved_tile_has_no_dimming_over_its_item_on_a_screen_only_on_the_hud() {
    let c = card(tile_with(|t| t.image.is_achieved = true));
    let colors = TeamColors::default();
    let fills_over = |gui: bool| {
        let d = draw_card(&CardView {
            card: &c,
            colors: &colors,
            now_ms: 0,
            show_outlines: false,
            team_outline: &|_| 0,
            items: &Names,
            images: None,
            team_name: None,
            is_gui: gui,
        });
        let item = d.iter().position(|x| matches!(x, Draw::Item { .. })).unwrap();
        d[item..].iter().filter(|x| matches!(x, Draw::Fill { w: 18, h: 18, .. })).count()
    };
    assert_eq!(fills_over(false), 1, "the HUD card dims the achieved tile over its item");
    assert_eq!(fills_over(true), 0, "a card on a screen does not (`!card.isGui`)");
}

/// Tile 6 (home (32, 42)) is the one the shuffle moved: its index reads slot 0 (12, 22), and tile 0 reads slot 6.
fn shuffled_card(shuffled_at: Option<u64>) -> ClientCard {
    let mut c = card(tile_with(|_| {}));
    c.shuffled_at = shuffled_at;
    c.shuffle_positions = (0..25).collect();
    c.shuffle_positions.swap(6, 0);
    c
}

fn item_at(d: &[Draw]) -> (f32, f32) {
    let found = items(d);
    assert_eq!(found.len(), 1, "one item on the card");
    (found[0].1, found[0].2)
}

fn assert_at(actual: (f32, f32), expected: (f32, f32)) {
    assert!((actual.0 - expected.0).abs() < 1e-4 && (actual.1 - expected.1).abs() < 1e-4, "got {actual:?}, expected {expected:?}");
}

#[test]
fn a_shuffled_tile_is_drawn_from_its_old_slot_to_its_home_while_the_shuffle_lasts() {
    let c = shuffled_card(Some(1_000));
    // at the shuffle the item is where the tile was: slot 0's corner (12, 22) plus the 1 px inset of the item
    assert_at(item_at(&draw(&c, 1_000, false)), (13.0, 23.0));
    // the eased quarter: e = InOut(0.25) = 0.1464466, the corner is 12 + 20 e and 22 + 20 e (not linear: 2.5 px at 0.25)
    assert_at(item_at(&draw(&c, 1_070, false)), (15.928_93, 25.928_93));
    // the half: the midpoint of (12, 22) and (32, 42)
    assert_at(item_at(&draw(&c, 1_140, false)), (23.0, 33.0));
    // the end and the window's padding after it: home
    assert_at(item_at(&draw(&c, 1_280, false)), (33.0, 43.0));
    assert_at(item_at(&draw(&c, 1_330, false)), (33.0, 43.0));
}

#[test]
fn a_fill_of_a_moving_tile_is_at_the_nearest_whole_pixel_while_its_item_is_not() {
    // at the quarter the corner is (14.93, 24.93): the achieved fill and its overlay go to (15, 25), the nearest pixel;
    // the item keeps the exact (15.93, 25.93). A floor would give (14, 24).
    let c = {
        let mut c = shuffled_card(Some(1_000));
        c.tiles[6] = tile_with(|t| t.image.is_achieved = true);
        c
    };
    let d = draw(&c, 1_070, false);
    assert_eq!(fills(&d), [(15, 25, 18, 18, TILE_ACHIEVED), (15, 25, 18, 18, 0x3f55_ff55)]);
    assert_at(item_at(&d), (15.928_93, 25.928_93));
}

#[test]
fn an_outline_of_a_moving_tile_is_drawn_at_the_nearest_whole_pixel() {
    let mut c = shuffled_card(Some(1_000));
    c.tiles[6].team_keys = vec!["a".into()];
    // one team: its outline's top edge runs from x = -1 to 18 at y = -1 of the tile, so from (15 - 1, 25 - 1) at the quarter
    let d = draw(&c, 1_070, true);
    assert!(fills(&d).contains(&(14, 24, 20, 1, 0xffff_5555)), "the top edge at the rounded corner");
}

#[test]
fn a_shuffle_that_is_over_draws_exactly_like_a_card_that_was_never_shuffled() {
    let never = draw(&card(tile_with(|_| {})), 1_330, true);
    assert_eq!(draw(&shuffled_card(Some(1_000)), 1_330, true), never, "the window is over at 1330 ms");
    assert_eq!(draw(&shuffled_card(Some(1_000)), 5_000, true), never);
    // and a card that was never shuffled ignores its positions altogether
    let mut scrambled = card(tile_with(|_| {}));
    scrambled.shuffle_positions = (0..25).rev().collect();
    assert_eq!(draw(&scrambled, 1_000, true), draw(&card(tile_with(|_| {})), 1_000, true));
}
