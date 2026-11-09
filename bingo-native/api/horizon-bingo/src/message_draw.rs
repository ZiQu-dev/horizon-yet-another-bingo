//! The score messages beside the card as a list of draw commands, ported from the original client's
//! `BingoMessageRenderer.kt` (`drawMessages`, `drawMessage`). Only the HUD path: `getMessageWrapped` is used by the
//! message history screen alone, so a HUD message is one unwrapped line at the default height of 18 pixels.
//!
//! Coordinates are GUI pixels relative to the anchor the original passes to `drawMessages`:
//! `(x, y + CARD_HEIGHT * cardScale * (1 - alignment.y))`. For a top alignment that is the card's bottom left corner,
//! for a bottom alignment the card's top left corner. The caller adds that anchor to every coordinate.
//!
//! Not drawn yet, listed as TODO(H2) below: the shadow of the text, the alpha of icons and sprites (`Draw` has no
//! alpha), the size of text under `messageScale` (`Draw::Text` has no scale), tile images that arrive as `card_image`
//! pictures, and the message-type icons' texture atlas. Nothing here is team-coloured: the original draws every
//! message in white, and so does this.

use crate::card_draw::{Draw, ItemNames, CARD_WIDTH};
use crate::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use crate::game_message::MessageType;
use crate::state::ScoreMessage;
use crate::text_plain::Lang;

/// `BingoMessageRenderer.ITEM_HEIGHT`: the height of a message without wrapping, and the spacing of the stack.
pub const ITEM_HEIGHT: i32 = 18;
/// `fadeInDuration`: 500 ms to appear, and as long again to disappear.
const FADE_MS: i64 = 500;
/// `reversed().take(5)`: at most five messages are drawn, the newest first.
const MAX_MESSAGES: usize = 5;
/// `Font.lineHeight` of the 26.3 font (`FontImpl.getTextHeight`).
const TEXT_HEIGHT: i32 = 9;

/// What the Kotlin reads from the client's state and config for one frame.
pub struct MessageView<'a> {
    /// `state.now`, in the host's milliseconds, on the same clock as `ScoreMessage::created_at`.
    pub now_ms: u64,
    /// `config.client.messageDurationSeconds`, default 5.
    pub duration_seconds: i64,
    /// `config.client.messageScale`, default 1.
    pub message_scale: f32,
    /// `options.chatScale`, default 1. The product is clamped to `0.01..=10`, as the original does.
    pub chat_scale: f32,
    /// `config.client.cardAlignment` as (x, y): `(0, 0)` is top left, the default.
    pub card_alignment: (i32, i32),
    /// `config.client.cardScale`, default 1.
    pub card_scale: f32,
    pub items: &'a dyn ItemNames,
    pub lang: Lang<'a>,
    /// `Font.getTextWidth` of a flattened line.
    pub text_width: &'a dyn Fn(&str) -> i32,
}

/// `Math.round` (half up), which `roundToInt` uses. Rust's `f32::round` rounds halves away from zero instead.
fn round_half_up(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// The draw commands of the score messages, in the order they are painted.
///
/// `messages` is the state's list in arrival order. A message older than its duration plus the fade is gone: an
/// expired one among the five newest is skipped as the original does, and after drawing every expired message is
/// removed from the list, whatever its rank, so old ones do not pile up. Messages still in their time stay as they are.
pub fn draw_messages(messages: &mut Vec<ScoreMessage>, view: &MessageView<'_>) -> Vec<Draw> {
    let scale = (view.message_scale * view.chat_scale).clamp(0.01, 10.0);
    let is_above_card = view.card_alignment.1 > 0;
    let is_left_of_card = view.card_alignment.0 > 0;
    let direction: f32 = if is_above_card { -1.0 } else { 1.0 };
    let duration = view.duration_seconds * 1000;
    let now = view.now_ms as i64;
    let lifetime = duration + FADE_MS;

    let mut out = Vec::new();
    let mut item_y: i32 = if is_above_card { -ITEM_HEIGHT } else { 0 };
    let shown: Vec<usize> = (0..messages.len()).rev().take(MAX_MESSAGES).collect();
    for index in shown {
        let since = now - messages[index].created_at as i64;
        let fade_in = ((since as f64 / FADE_MS as f64) as f32).clamp(0.0, 1.0);
        let fade_out = ((1.0 - ((since - duration) as f64 / FADE_MS as f64) as f32)).clamp(0.0, 1.0);
        let visibility = fade_in * fade_out;

        if since > lifetime {
            continue;
        }

        // The message rises (or sinks) into place as it fades out: (1 - fadeOut)^2 of its height.
        let lift = (1.0 - fade_out).powi(2) * ITEM_HEIGHT as f32 * direction;
        let origin = (
            if is_left_of_card { CARD_WIDTH as f32 * view.card_scale } else { 0.0 },
            scale * (item_y as f32 + lift),
        );
        draw_message(&mut out, &messages[index], visibility, is_left_of_card, origin, scale, view);

        // The stack moves by the message's height times fadeIn^2, so a new message pushes the others in gradually.
        item_y += round_half_up(ITEM_HEIGHT as f32 * direction * fade_in.powi(2));
    }
    messages.retain(|m| now - m.created_at as i64 <= lifetime);
    out
}

fn draw_message(
    out: &mut Vec<Draw>,
    message: &ScoreMessage,
    visibility: f32,
    is_left_of_card: bool,
    origin: (f32, f32),
    scale: f32,
    view: &MessageView<'_>,
) {
    // `alpha = visibility.pow(2)`: it sets the text colour here.
    // TODO(H2): the original also multiplies the icon's shader colour by alpha (`setShaderColor`); `Draw` has no alpha
    // for items and sprites, so icons are drawn opaque. Add an alpha field to `Draw` to close this.
    let alpha = visibility.powi(2);
    // Local units of the message, scaled by `messageScale` and moved by the origin (`matrices`).
    let at = |lx: f32, ly: f32| (origin.0 + scale * lx, origin.1 + scale * ly);

    let item_x: f32 = if is_left_of_card { (-8 - ITEM_HEIGHT) as f32 } else { 8.0 };
    let item_y: f32 = (ITEM_HEIGHT / 2 - 8) as f32;
    let packet = &message.packet;
    if packet.image.item.is_some() || packet.image.texture.is_some() {
        let (x, y) = at(item_x, item_y);
        tile_icon(out, view.items, &packet.image, &packet.image_list, x, y, scale);
        decoration(out, &packet.image, x, y, scale);
    } else {
        // `drawGuiTexture` of the message type's icon, 16 x 16.
        let name = match packet.message_type {
            MessageType::LeadingTeam => "bingo/message_leading",
            MessageType::LineScored | MessageType::CardCompleted => "bingo/message_check",
            MessageType::ItemScored => "bingo/message_item_hidden",
        };
        let (x, y) = at(item_x, item_y);
        out.push(Draw::Sprite { name: name.into(), x, y, w: 16.0 * scale, h: 16.0 * scale });
    }

    // `messageWidth` is the widest line; a HUD message has one line.
    let styled = crate::text_plain::runs(&packet.message, view.lang);
    let width: i32 = styled.iter().map(|r| (view.text_width)(&r.text)).sum();
    let text_x: f32 = if is_left_of_card { (-28 - width) as f32 } else { 28.0 };
    // `textY = height/2 - (lines * textHeight + (lines - 1) * 2) / 2`, one line: 9 - 4 = 5.
    let text_y: f32 = (ITEM_HEIGHT / 2 - TEXT_HEIGHT / 2) as f32;
    let (x, y) = at(text_x, text_y);
    // `rgba(255, 255, 255, (alpha * 255).toInt().coerceIn(5, 255))`, drawn with a shadow.
    // TODO(H2): `Draw::Text` has neither a shadow nor a scale, so under `messageScale` != 1 the text keeps its size and
    // only its position is scaled (rounded to whole pixels).
    let a = ((alpha * 255.0) as i32).clamp(5, 255) as u32;
    // The runs follow one another; a run's own colour replaces the white.
    let mut run_x = x.round() as i32;
    for run in styled {
        let w = (view.text_width)(&run.text);
        match run.head {
            // A player's head: 8 x 8 where the two spaces are.
            Some(uuid) => out.push(Draw::Sprite {
                name: crate::card_images::face_sprite(uuid),
                x: run_x as f32,
                y: y.round(),
                w: 8.0,
                h: 8.0,
            }),
            None => out.push(Draw::Text {
                x: run_x,
                y: y.round() as i32,
                text: run.text,
                argb: (a << 24) | run.rgb.unwrap_or(0x00ff_ffff),
            }),
        }
        run_x += w;
    }
}

/// `CardTileRenderer.renderTile` for a message: the item, or the one-of / all-of stack of a list. The offsets are
/// the same as the card's (`card_draw`), which are the original's `translate` and `scale` values.
fn tile_icon(
    out: &mut Vec<Draw>,
    items: &dyn ItemNames,
    image: &CardTileImage,
    list: &[CardTileImage],
    x: f32,
    y: f32,
    scale: f32,
) {
    let one = |out: &mut Vec<Draw>, img: &CardTileImage, x: f32, y: f32, size: f32| {
        // TODO(H2): a tile image with a `texture` (a `card_image` picture) is drawn by the original; Draw has no
        // dynamic texture, so only the item is drawn here.
        if let Some(stack) = &img.item {
            if let Some(name) = items.item_name(stack.item) {
                out.push(Draw::Item { name, x, y, size: size * scale });
            }
        }
    };
    if list.len() > 1 && image.item_style == ItemStyle::OneOf {
        // scale 12/16; each next image 2 px further (4 / scale / 2 in scaled units), the first on top: draw it last.
        for i in (0..3).rev() {
            if let Some(img) = list.get(i) {
                let step = 2.0 * i as f32;
                one(out, img, x + scale * step, y + scale * step, 12.0);
            }
        }
    } else if list.len() > 1 && image.item_style == ItemStyle::AllOf {
        // scale 9/16: a 2 x 2 grid of 9 px icons whose cells are 12 scaled units (6.75 px) apart.
        for i in 0..4 {
            if let Some(img) = list.get(i) {
                let cx = x + scale * (i % 2) as f32 * 6.75;
                let cy = y + scale * (i / 2) as f32 * 6.75;
                one(out, img, cx, cy, 9.0);
            }
        }
    } else {
        one(out, image, x, y, 16.0);
    }
}

/// `CardTileRenderer.renderTileDecorations`: a 20 x 20 frame two pixels up and left of the item.
fn decoration(out: &mut Vec<Draw>, image: &CardTileImage, x: f32, y: f32, scale: f32) {
    let name = match (&image.decoration_texture, image.decoration) {
        (Some(custom), _) => custom.rsplit(':').next().unwrap_or(custom).to_string(),
        (None, Decoration::Advancement) => "bingo/image_frame_advancement".to_string(),
        (None, Decoration::Forbidden) => "bingo/image_frame_forbidden".to_string(),
        (None, Decoration::FreeSpace) => "bingo/image_frame_free_space".to_string(),
        (None, Decoration::MultiItem | Decoration::None) => return,
    };
    out.push(Draw::Sprite {
        name,
        x: x - 2.0 * scale,
        y: y - 2.0 * scale,
        w: 20.0 * scale,
        h: 20.0 * scale,
    });
}
