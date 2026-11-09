//! The score messages against the numbers in the original renderer (BingoMessageRenderer.kt). Every expected value is
//! worked out by hand from the Kotlin: ITEM_HEIGHT 18, a 500 ms fade, 5 seconds shown, text 28 px from the icon, the
//! icon 8 px from the card, a 9 px font line, and the card 122 px wide.
use horizon_bingo::card_draw::{Draw, ItemNames, CARD_WIDTH};
use horizon_bingo::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use horizon_bingo::game_message::{GameMessagePacket, MessageType};
use horizon_bingo::game_status::JavaDuration;
use horizon_bingo::item::ItemStack;
use horizon_bingo::message_draw::{draw_messages, MessageView};
use horizon_bingo::state::ScoreMessage;
use horizon_bingo::text::Text;

struct Names;
impl ItemNames for Names {
    fn item_name(&self, id: i32) -> Option<String> {
        (id != 999).then(|| format!("item_{id}"))
    }
}

/// A font where every character is 6 px wide.
fn width6(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

fn no_lang(_: &str) -> Option<String> {
    None
}

fn stack(id: i32) -> ItemStack {
    ItemStack { count: 1, item: id, added: vec![], removed: vec![] }
}

fn msg(created_at: u64, text: &str, kind: MessageType) -> ScoreMessage {
    ScoreMessage {
        created_at,
        packet: GameMessagePacket {
            id: created_at as u128 + 1,
            time_elapsed: JavaDuration::of_millis(0),
            team: "bingo_red".into(),
            image: CardTileImage::default(),
            image_list: Vec::new(),
            message_type: kind,
            message: Text::plain(text),
            is_update: false,
        },
    }
}

/// A message whose icon is item `id`.
fn item_msg(created_at: u64, text: &str, id: i32) -> ScoreMessage {
    let mut m = msg(created_at, text, MessageType::ItemScored);
    m.packet.image.item = Some(stack(id));
    m
}

/// The default config: 5 seconds, scale 1, top left, card scale 1, no chat scaling.
fn view(now_ms: u64) -> MessageView<'static> {
    static NAMES: Names = Names;
    MessageView {
        now_ms,
        duration_seconds: 5,
        message_scale: 1.0,
        chat_scale: 1.0,
        card_alignment: (0, 0),
        card_scale: 1.0,
        items: &NAMES,
        lang: &no_lang,
        text_width: &width6,
    }
}

fn texts(d: &[Draw]) -> Vec<(i32, i32, String, u32)> {
    d.iter()
        .filter_map(|c| match c {
            Draw::Text { x, y, text, argb } => Some((*x, *y, text.clone(), *argb)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_five_newest_are_drawn_newest_first_each_18_px_lower_and_the_sixth_is_not_drawn() {
    // created at 0, now 1000: fully shown (fadeIn 2 -> 1, fadeOut 1), alpha 255. Text at 28 px, 5 px below the stack.
    let mut messages: Vec<ScoreMessage> = (0..6).map(|i| msg(0, &format!("m{i}"), MessageType::ItemScored)).collect();
    let d = draw_messages(&mut messages, &view(1_000));
    assert_eq!(
        texts(&d),
        [
            (28, 5, "m5".into(), 0xffff_ffff),
            (28, 23, "m4".into(), 0xffff_ffff),
            (28, 41, "m3".into(), 0xffff_ffff),
            (28, 59, "m2".into(), 0xffff_ffff),
            (28, 77, "m1".into(), 0xffff_ffff),
        ],
    );
    // the sixth-oldest is past the five shown, and not expired yet: it stays in the list
    assert_eq!(messages.len(), 6);
}

#[test]
fn over_a_card_at_the_bottom_the_stack_grows_upward_from_minus_18() {
    let mut messages = vec![msg(0, "old", MessageType::ItemScored), msg(0, "new", MessageType::ItemScored)];
    let mut v = view(1_000);
    v.card_alignment = (0, 1);
    let d = draw_messages(&mut messages, &v);
    // the newest starts at -18 (its text at -13), the next one at -36 (text at -31)
    assert_eq!(texts(&d), [(28, -13, "new".into(), 0xffff_ffff), (28, -31, "old".into(), 0xffff_ffff)]);
}

#[test]
fn a_message_fades_in_over_half_a_second_and_pushes_the_stack_by_the_rounded_offset() {
    // older: created at 0, shown at now 1000. newer: created at 750, 250 ms old -> fadeIn 0.5.
    // newer: visibility 0.5, alpha 0.25 -> 0.25 * 255 = 63.75 -> 63 = 0x3f. Its stack step is round(18 * 0.25) = round(4.5) = 5.
    let mut messages = vec![msg(0, "older", MessageType::ItemScored), msg(750, "newer", MessageType::ItemScored)];
    let d = draw_messages(&mut messages, &view(1_000));
    assert_eq!(
        texts(&d),
        [(28, 5, "newer".into(), 0x3fff_ffff), (28, 10, "older".into(), 0xffff_ffff)],
    );
}

#[test]
fn a_message_fading_out_sinks_by_the_square_of_its_fade_and_dims() {
    // 5375 ms old: fadeOut = 1 - (5375 - 5000) / 500 = 0.25, visibility 0.25, alpha 0.0625 -> 15.94 -> 15 = 0x0f.
    // It sinks by (1 - 0.25)^2 * 18 = 10.125, so its text is at 5 + 10.125 = 15.125, drawn at 15.
    let mut messages = vec![msg(0, "fading", MessageType::ItemScored)];
    let d = draw_messages(&mut messages, &view(5_375));
    assert_eq!(texts(&d), [(28, 15, "fading".into(), 0x0fff_ffff)]);
}

#[test]
fn a_message_is_removed_only_after_its_duration_and_fade_have_both_passed() {
    // duration 5000 + fade 500 = 5500. At exactly 5500 it is still drawn (alpha clamped up to 5), fully sunk by
    // (1 - 0)^2 * 18 = 18 px: text at 5 + 18 = 23. It is removed at 5501.
    let mut at_limit = vec![msg(0, "m", MessageType::ItemScored)];
    let d = draw_messages(&mut at_limit, &view(5_500));
    assert_eq!(texts(&d), [(28, 23, "m".into(), 0x05ff_ffff)]);
    assert_eq!(at_limit.len(), 1);

    let mut past = vec![msg(0, "m", MessageType::ItemScored)];
    let d = draw_messages(&mut past, &view(5_501));
    assert!(texts(&d).is_empty());
    assert!(past.is_empty(), "the expired message leaves the list");
}

#[test]
fn on_the_left_of_the_card_the_text_ends_28_px_before_the_icon_and_the_icon_is_18_px_left_of_it() {
    // card width 122. Icon at 122 - 8 - 18 = 96. Text: "Red Team" is 48 px, so it starts at 122 - 28 - 48 = 46.
    let mut messages = vec![msg(0, "Red Team", MessageType::ItemScored)];
    let mut v = view(1_000);
    v.card_alignment = (1, 0);
    let d = draw_messages(&mut messages, &v);
    assert_eq!(texts(&d), [(46, 5, "Red Team".into(), 0xffff_ffff)]);
    assert!(d.contains(&Draw::Sprite { name: "bingo/message_item_hidden".into(), x: 96.0, y: 1.0, w: 16.0, h: 16.0 }));
    assert_eq!(CARD_WIDTH, 122);
}

#[test]
fn card_scale_moves_the_messages_on_the_left_but_not_their_size() {
    // the card is 122 * 2 = 244 px wide; the text is 28 px from the icon, the icon 8 px from the card.
    let mut messages = vec![msg(0, "ab", MessageType::ItemScored)]; // 12 px
    let mut v = view(1_000);
    v.card_alignment = (1, 0);
    v.card_scale = 2.0;
    let d = draw_messages(&mut messages, &v);
    // text x = 244 - 28 - 12 = 204; icon x = 244 - 26 = 218, still 16 px
    assert_eq!(texts(&d), [(204, 5, "ab".into(), 0xffff_ffff)]);
    assert!(d.contains(&Draw::Sprite { name: "bingo/message_item_hidden".into(), x: 218.0, y: 1.0, w: 16.0, h: 16.0 }));
}

#[test]
fn message_scale_multiplies_the_offsets_and_the_stack_step() {
    // scale 2: the text at (28, 5) becomes (56, 10); the icon at (8, 1) becomes (16, 2) and 32 px square.
    // The second message steps the stack by 18 local units, which is 36 px: its text is at (56, 46).
    let mut messages = vec![item_msg(0, "a", 7), item_msg(0, "b", 8)];
    let mut v = view(1_000);
    v.message_scale = 2.0;
    let d = draw_messages(&mut messages, &v);
    assert_eq!(texts(&d), [(56, 10, "b".into(), 0xffff_ffff), (56, 46, "a".into(), 0xffff_ffff)]);
    assert!(d.contains(&Draw::Item { name: "item_8".into(), x: 16.0, y: 2.0, size: 32.0 }));
}

#[test]
fn the_scale_is_the_config_times_the_chat_scale_clamped_to_0_01_through_10() {
    let mut messages = vec![item_msg(0, "a", 7)];
    let mut v = view(1_000);
    v.message_scale = 2.0;
    v.chat_scale = 0.5; // product 1: the icon stays 16 px
    let d = draw_messages(&mut messages, &v);
    assert!(d.contains(&Draw::Item { name: "item_7".into(), x: 8.0, y: 1.0, size: 16.0 }));

    let mut big = vec![item_msg(0, "a", 7)];
    let mut v = view(1_000);
    v.message_scale = 50.0; // clamped to 10: the icon is 160 px at (80, 10)
    let d = draw_messages(&mut big, &v);
    assert!(d.contains(&Draw::Item { name: "item_7".into(), x: 80.0, y: 10.0, size: 160.0 }));
}

#[test]
fn an_item_is_drawn_at_8_1_and_its_decoration_frame_two_pixels_up_and_left_of_it() {
    let mut m = item_msg(0, "x", 7);
    m.packet.image.decoration = Decoration::Advancement;
    let mut messages = vec![m];
    let d = draw_messages(&mut messages, &view(1_000));
    // the item, then its frame (drawn after it), then the text
    assert_eq!(d[0], Draw::Item { name: "item_7".into(), x: 8.0, y: 1.0, size: 16.0 });
    assert_eq!(
        d[1],
        Draw::Sprite { name: "bingo/image_frame_advancement".into(), x: 6.0, y: -1.0, w: 20.0, h: 20.0 },
    );
    assert_eq!(d.len(), 3);
}

#[test]
fn a_one_of_list_is_three_12_px_items_two_pixels_apart_with_the_first_drawn_last() {
    let mut m = item_msg(0, "x", 1);
    m.packet.image.item_style = ItemStyle::OneOf;
    m.packet.image_list = vec![
        CardTileImage { item: Some(stack(1)), ..CardTileImage::default() },
        CardTileImage { item: Some(stack(2)), ..CardTileImage::default() },
        CardTileImage { item: Some(stack(3)), ..CardTileImage::default() },
    ];
    let mut messages = vec![m];
    let d = draw_messages(&mut messages, &view(1_000));
    let items: Vec<&Draw> = d.iter().filter(|c| matches!(c, Draw::Item { .. })).collect();
    assert_eq!(
        items,
        [
            &Draw::Item { name: "item_3".into(), x: 12.0, y: 5.0, size: 12.0 },
            &Draw::Item { name: "item_2".into(), x: 10.0, y: 3.0, size: 12.0 },
            &Draw::Item { name: "item_1".into(), x: 8.0, y: 1.0, size: 12.0 },
        ],
    );
}

#[test]
fn an_all_of_list_is_a_2_by_2_grid_of_9_px_icons_6_75_px_apart() {
    let mut m = item_msg(0, "x", 1);
    m.packet.image.item_style = ItemStyle::AllOf;
    m.packet.image_list = (1..=4).map(|i| CardTileImage { item: Some(stack(i)), ..CardTileImage::default() }).collect();
    let mut messages = vec![m];
    let d = draw_messages(&mut messages, &view(1_000));
    let items: Vec<&Draw> = d.iter().filter(|c| matches!(c, Draw::Item { .. })).collect();
    // 12 scaled units * 9/16 = 6.75
    assert_eq!(
        items,
        [
            &Draw::Item { name: "item_1".into(), x: 8.0, y: 1.0, size: 9.0 },
            &Draw::Item { name: "item_2".into(), x: 14.75, y: 1.0, size: 9.0 },
            &Draw::Item { name: "item_3".into(), x: 8.0, y: 7.75, size: 9.0 },
            &Draw::Item { name: "item_4".into(), x: 14.75, y: 7.75, size: 9.0 },
        ],
    );
}

#[test]
fn an_expired_message_beyond_the_five_newest_is_removed_too() {
    // now 6000: "old" is 6000 ms old, past 5000 + 500 = 5500, and it is the sixth newest, so it is not drawn. Before, it
    // stayed in the list for ever. The five fresh ones are at full strength, 18 px apart from the newest (y 5) down.
    let mut messages = vec![msg(0, "old", MessageType::ItemScored)];
    for (created, text) in [(1_000, "m1"), (2_000, "m2"), (3_000, "m3"), (4_000, "m4"), (5_000, "m5")] {
        messages.push(msg(created, text, MessageType::ItemScored));
    }
    let d = draw_messages(&mut messages, &view(6_000));
    assert_eq!(
        texts(&d),
        [
            (28, 5, "m5".into(), 0xffff_ffff),
            (28, 23, "m4".into(), 0xffff_ffff),
            (28, 41, "m3".into(), 0xffff_ffff),
            (28, 59, "m2".into(), 0xffff_ffff),
            (28, 77, "m1".into(), 0xffff_ffff),
        ],
    );
    assert_eq!(messages.len(), 5, "the expired sixth message is gone");
    assert!(messages.iter().all(|m| m.created_at != 0), "no message of the expired one's time is left");
}

#[test]
fn pruning_beyond_the_five_does_not_change_what_the_five_show() {
    // now 6000. "dead" (created at 0) is expired and among the five newest: it is skipped and removed, as before, so the
    // stack below it is not moved. "old" (created at 1000) is still in its time and sixth newest: not drawn, kept.
    let mut messages = vec![msg(1_000, "old", MessageType::ItemScored), msg(0, "dead", MessageType::ItemScored)];
    for (created, text) in [(2_000, "m2"), (3_000, "m3"), (4_000, "m4"), (5_000, "m5")] {
        messages.push(msg(created, text, MessageType::ItemScored));
    }
    let d = draw_messages(&mut messages, &view(6_000));
    assert_eq!(
        texts(&d),
        [
            (28, 5, "m5".into(), 0xffff_ffff),
            (28, 23, "m4".into(), 0xffff_ffff),
            (28, 41, "m3".into(), 0xffff_ffff),
            (28, 59, "m2".into(), 0xffff_ffff),
        ],
    );
    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0].created_at, 1_000, "the message still in its time stays, even past the five");
}

#[test]
fn a_message_without_an_item_shows_the_icon_of_its_type_at_8_1_and_16_px() {
    let icon = |kind| {
        let mut messages = vec![msg(0, "x", kind)];
        let d = draw_messages(&mut messages, &view(1_000));
        match &d[0] {
            Draw::Sprite { name, x, y, w, h } => (name.clone(), *x, *y, *w, *h),
            other => panic!("expected the icon first, got {other:?}"),
        }
    };
    assert_eq!(icon(MessageType::LeadingTeam), ("bingo/message_leading".into(), 8.0, 1.0, 16.0, 16.0));
    assert_eq!(icon(MessageType::LineScored), ("bingo/message_check".into(), 8.0, 1.0, 16.0, 16.0));
    assert_eq!(icon(MessageType::CardCompleted), ("bingo/message_check".into(), 8.0, 1.0, 16.0, 16.0));
    assert_eq!(icon(MessageType::ItemScored), ("bingo/message_item_hidden".into(), 8.0, 1.0, 16.0, 16.0));
}
