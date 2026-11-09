//! One draw list for the four screens. `screen_main`, `screen_cards`, `screen_end` and `screen_placement` each have
//! their own command enum, all of them wrapping `card_draw::Draw`; `UiDraw` is the common form. The `From` impls
//! convert each screen's commands into it, `nine_slice` cuts a texture into the nine pieces a stretched box needs,
//! and `run` executes a list through the `Host` trait, sprites by their rectangle in the mod's atlas.
//!
//! Positions are GUI pixels. Fractional positions (text on a rotated line, a fill scaled by a screen's animation) are
//! kept as they are here and rounded only when `run` hands them to the host, whose calls take whole pixels.

use crate::atlas::Atlas;
use crate::card_draw::Draw;
use crate::host::Host;
use crate::screen_cards::ScreenDraw as CardsDraw;
use crate::screen_end::ScreenDraw as EndDraw;
use crate::screen_main::ScreenDraw as MainDraw;
use crate::screen_placement::PlacementDraw;

#[derive(Debug, Clone, PartialEq)]
pub enum UiDraw {
    /// A texture of the mod's atlas, stretched into the rectangle.
    Sprite { name: String, x: f32, y: f32, w: f32, h: f32 },
    /// A rectangle of one colour. Its edges are rounded to whole pixels when drawn; a rectangle with no area draws nothing.
    Fill { x: f32, y: f32, w: f32, h: f32, argb: u32 },
    /// Text whose top left corner is at (x, y), rounded when drawn.
    // TODO(integration): `obfuscated` text should draw random glyphs of the same width; the host has no such call.
    Text { x: f32, y: f32, text: String, argb: u32, shadow: bool, obfuscated: bool },
    /// An item icon by name, `size` pixels square, top left at (x, y).
    Item { name: String, x: f32, y: f32, size: f32 },
    /// Text in the frame of `matrices.translate(anchor); rotate(rotation); scale(scale)`: its top left is at `offset` in
    /// that frame, so the glyphs are scaled and turned with it (`rotation` in radians, counter-clockwise).
    TextXf { text: String, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, argb: u32, shadow: bool },
    /// A square of half side `half` centred on (`cx`, `cy`), turned by `rotation` radians.
    SquareXf { cx: f32, cy: f32, half: f32, rotation: f32, argb: u32 },
    /// A stack count, white with a shadow, right-aligned at `right`.
    Count { text: String, right: i32, y: i32 },
    /// A texture cut in nine: `slice` pixels stay at each edge. `tex` is the texture's size in pixels; `None` means the
    /// sprite's size in the atlas, which is its size as a texture.
    NineSlice { name: String, x: i32, y: i32, w: i32, h: i32, slice: i32, tex: Option<(i32, i32)> },
    /// A tooltip box with these lines, its corner at (x, y).
    Tooltip { x: i32, y: i32, lines: Vec<String> },
}

/// Whole pixel of a coordinate: halves round up, as the Kotlin `Math.round` does for positive values.
fn px(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

impl From<&Draw> for UiDraw {
    fn from(d: &Draw) -> UiDraw {
        match d {
            Draw::Sprite { name, x, y, w, h } => UiDraw::Sprite { name: name.clone(), x: *x, y: *y, w: *w, h: *h },
            Draw::Fill { x, y, w, h, argb } => {
                UiDraw::Fill { x: *x as f32, y: *y as f32, w: *w as f32, h: *h as f32, argb: *argb }
            }
            // The card's own text has no shadow flag, so it is drawn plain.
            Draw::Text { x, y, text, argb } => UiDraw::Text {
                x: *x as f32,
                y: *y as f32,
                text: text.clone(),
                argb: *argb,
                shadow: false,
                obfuscated: false,
            },
            Draw::Item { name, x, y, size } => UiDraw::Item { name: name.clone(), x: *x, y: *y, size: *size },
            Draw::Count { text, right, y } => UiDraw::Count { text: text.clone(), right: *right, y: *y },
        }
    }
}

impl From<&MainDraw> for UiDraw {
    fn from(d: &MainDraw) -> UiDraw {
        match d {
            MainDraw::Base(b) => UiDraw::from(b),
            MainDraw::NinePatch { name, x, y, w, h, slice, texture_w, texture_h } => UiDraw::NineSlice {
                name: name.clone(),
                x: *x,
                y: *y,
                w: *w,
                h: *h,
                slice: *slice,
                tex: Some((*texture_w, *texture_h)),
            },
            // `matrices.translate(anchor)`, `rotate(rotation)`, then the text at `scale * offset` in that frame: the
            // offset is already centred by the module (`-width / 2`), so only the scale and the turn are applied here.
            // The rotation is counter-clockwise in the Kotlin's `rotate`, with the usual x / y axes: (x, y) turns to
            // (x cos r - y sin r, x sin r + y cos r).
            MainDraw::Label { text, anchor, offset, scale, rotation, argb, shadow } => UiDraw::TextXf {
                text: text.clone(),
                anchor: *anchor,
                offset: *offset,
                scale: *scale,
                rotation: *rotation,
                argb: *argb,
                shadow: *shadow,
            },
            MainDraw::Square { cx, cy, half, rotation, argb } => {
                UiDraw::SquareXf { cx: *cx, cy: *cy, half: *half, rotation: *rotation, argb: *argb }
            }
        }
    }
}

impl From<&CardsDraw> for UiDraw {
    fn from(d: &CardsDraw) -> UiDraw {
        match d {
            CardsDraw::Plain(p) => UiDraw::from(p),
            // The widget's box has no texture size of its own: the atlas knows the sprite's size.
            CardsDraw::NinePatch { name, x, y, w, h, slice } => UiDraw::NineSlice {
                name: name.clone(),
                x: *x,
                y: *y,
                w: *w,
                h: *h,
                slice: *slice,
                tex: None,
            },
            CardsDraw::Label { text, anchor, offset, scale, rotation, argb, shadow } => UiDraw::TextXf {
                text: text.clone(),
                anchor: *anchor,
                offset: *offset,
                scale: *scale,
                rotation: *rotation,
                argb: *argb,
                shadow: *shadow,
            },
            // `matrices.scale(scale)` around the text: the position is already scaled, the glyphs scale about it.
            CardsDraw::ScaledText { x, y, scale, text, argb } => UiDraw::TextXf {
                text: text.clone(),
                anchor: (0.0, 0.0),
                offset: (*x, *y),
                scale: *scale,
                rotation: 0.0,
                argb: *argb,
                shadow: false,
            },
        }
    }
}

impl From<&EndDraw> for UiDraw {
    fn from(d: &EndDraw) -> UiDraw {
        match d {
            EndDraw::Base(b) => UiDraw::from(b),
            EndDraw::Text { x, y, text, argb, shadow, obfuscated } => UiDraw::Text {
                x: *x as f32,
                y: *y as f32,
                text: text.clone(),
                argb: *argb,
                shadow: *shadow,
                obfuscated: *obfuscated,
            },
            EndDraw::Sprite { name, x, y, w, h } => {
                UiDraw::Sprite { name: name.clone(), x: *x as f32, y: *y as f32, w: *w as f32, h: *h as f32 }
            }
            EndDraw::NineSlice { name, x, y, w, h, slice, tex_w, tex_h } => UiDraw::NineSlice {
                name: name.clone(),
                x: *x,
                y: *y,
                w: *w,
                h: *h,
                slice: *slice,
                tex: Some((*tex_w, *tex_h)),
            },
            // TODO(integration): a player's head before its name is not drawn; the host's tooltip takes text only.
            EndDraw::Tooltip { x, y, lines } => UiDraw::Tooltip {
                x: *x,
                y: *y,
                lines: lines.iter().map(|l| l.text.clone()).collect(),
            },
        }
    }
}

impl From<&PlacementDraw> for UiDraw {
    fn from(d: &PlacementDraw) -> UiDraw {
        match d {
            PlacementDraw::Draw(b) => UiDraw::from(b),
            PlacementDraw::ScaledFill { x, y, w, h, argb } => {
                UiDraw::Fill { x: *x, y: *y, w: *w, h: *h, argb: *argb }
            }
        }
    }
}

/// Splits a texture into the nine pieces that draw a `dst` box: `src` is the texture's rectangle in the atlas
/// `[x, y, w, h]`, `tex` the texture's size in its own pixels, `slice` the border in those pixels. Returns
/// `(src, dst)` pairs in the order top left, top, top right, left, centre, right, bottom left, bottom, bottom right,
/// each `[x, y, w, h]`. The corners keep their size; the edges stretch along one axis and the centre along both.
/// A border is shrunk to half the box when the box is smaller than twice the slice (as vanilla does), and a piece with
/// no area is left out, so a box of exactly twice the slice has no centre.
pub fn nine_slice(src: [f32; 4], tex: [f32; 2], slice: f32, dst: [f32; 4]) -> Vec<([f32; 4], [f32; 4])> {
    let [dx, dy, dw, dh] = dst;
    if dw <= 0.0 || dh <= 0.0 || tex[0] <= 0.0 || tex[1] <= 0.0 {
        return Vec::new();
    }
    // Texture pixels to atlas pixels.
    let (kx, ky) = (src[2] / tex[0], src[3] / tex[1]);
    // The texture's own borders, kept inside the texture.
    let (bx, by) = (slice.min(tex[0] / 2.0).max(0.0), slice.min(tex[1] / 2.0).max(0.0));
    // The box's borders, shrunk when the box is smaller than twice the slice.
    let (cx, cy) = (slice.min(dw / 2.0).max(0.0), slice.min(dh / 2.0).max(0.0));

    let xs_src = [0.0, bx, tex[0] - bx, tex[0]];
    let ys_src = [0.0, by, tex[1] - by, tex[1]];
    let xs_dst = [dx, dx + cx, dx + dw - cx, dx + dw];
    let ys_dst = [dy, dy + cy, dy + dh - cy, dy + dh];

    let mut out = Vec::with_capacity(9);
    for j in 0..3 {
        for i in 0..3 {
            let (sw, sh) = (xs_src[i + 1] - xs_src[i], ys_src[j + 1] - ys_src[j]);
            let (ow, oh) = (xs_dst[i + 1] - xs_dst[i], ys_dst[j + 1] - ys_dst[j]);
            if sw <= 0.0 || sh <= 0.0 || ow <= 0.0 || oh <= 0.0 {
                continue;
            }
            out.push((
                [src[0] + xs_src[i] * kx, src[1] + ys_src[j] * ky, sw * kx, sh * ky],
                [xs_dst[i], ys_dst[j], ow, oh],
            ));
        }
    }
    out
}

/// `IDrawService.drawNinePatch`: the original tiles the centre and the edges in pieces of the texture's own centre
/// size instead of stretching them. Texture sizes `tex` and `slice` are in texture pixels; `src` is the sprite's
/// rectangle in the atlas (scaled by `src / tex` when the atlas holds it at another size); `dst` is `[x, y, w, h]`.
/// Pieces come in the Kotlin's order: the centre, the horizontal edges, the vertical edges, the corners.
pub fn nine_patch(src: [f32; 4], tex: [f32; 2], slice: f32, dst: [f32; 4]) -> Vec<([f32; 4], [f32; 4])> {
    let [x, y, w, h] = dst;
    let (kx, ky) = (src[2] / tex[0], src[3] / tex[1]);
    let (tcw, tch) = (tex[0] - slice * 2.0, tex[1] - slice * 2.0);
    let (cw, ch) = (w - slice * 2.0, h - slice * 2.0);
    if tcw <= 0.0 || tch <= 0.0 {
        return Vec::new();
    }
    let parts = |len: f32, step: f32| (len / step).ceil().max(0.0) as i32;
    let (xp, yp) = (parts(cw, tcw), parts(ch, tch));
    let piece = |u: f32, v: f32, pw: f32, ph: f32, px: f32, py: f32| {
        ([src[0] + u * kx, src[1] + v * ky, pw * kx, ph * ky], [px, py, pw, ph])
    };
    let mut out = Vec::new();
    for i in 0..xp {
        let pw = (cw - i as f32 * tcw).min(tcw);
        for j in 0..yp {
            let ph = (ch - j as f32 * tch).min(tch);
            out.push(piece(slice, slice, pw, ph, x + slice + i as f32 * tcw, y + slice + j as f32 * tch));
        }
    }
    for i in 0..xp {
        let pw = (cw - i as f32 * tcw).min(tcw);
        out.push(piece(slice, 0.0, pw, slice, x + slice + i as f32 * tcw, y));
        out.push(piece(slice, tex[1] - slice, pw, slice, x + slice + i as f32 * tcw, y + h - slice));
    }
    for j in 0..yp {
        let ph = (ch - j as f32 * tch).min(tch);
        out.push(piece(0.0, slice, slice, ph, x, y + slice + j as f32 * tch));
        out.push(piece(tex[0] - slice, slice, slice, ph, x + w - slice, y + slice + j as f32 * tch));
    }
    out.push(piece(0.0, 0.0, slice, slice, x, y));
    out.push(piece(tex[0] - slice, 0.0, slice, slice, x + w - slice, y));
    out.push(piece(0.0, tex[1] - slice, slice, slice, x, y + h - slice));
    out.push(piece(tex[0] - slice, tex[1] - slice, slice, slice, x + w - slice, y + h - slice));
    out
}

/// Executes a draw list through the host. Sprites are looked up in the atlas (skipped when it lacks them); a nine-slice
/// is cut into `draw_sprite` calls when the atlas has it, else the host draws its vanilla widget of that name.
pub fn run(cmds: &[UiDraw], atlas: &Atlas, host: &mut dyn Host) {
    for cmd in cmds {
        match cmd {
            UiDraw::Sprite { name, x, y, w, h } => {
                if let Some(src) = atlas.rect(name) {
                    host.draw_sprite(src, [*x, *y, *w, *h]);
                }
            }
            UiDraw::Fill { x, y, w, h, argb } => {
                let (x0, y0) = (px(*x), px(*y));
                let (x1, y1) = (px(x + w), px(y + h));
                if x1 > x0 && y1 > y0 {
                    host.fill_rect(x0, y0, x1 - x0, y1 - y0, *argb);
                }
            }
            UiDraw::Text { x, y, text, argb, shadow, obfuscated: _ } => {
                // TODO(integration): the host draws text at scale 1 only; obfuscated text keeps its string.
                host.draw_text(px(*x), px(*y), text, *argb, *shadow);
            }
            UiDraw::Item { name, x, y, size } => host.draw_item(name, *x, *y, *size),
            UiDraw::TextXf { text, anchor, offset, scale, rotation, argb, shadow } => {
                host.draw_text_xf(text, *anchor, *offset, *scale, *rotation, *argb, *shadow)
            }
            UiDraw::SquareXf { cx, cy, half, rotation, argb } => host.fill_square_xf(*cx, *cy, *half, *rotation, *argb),
            UiDraw::Count { text, right, y } => host.draw_text(right - host.text_width(text), *y, text, 0xffff_ffff, true),
            UiDraw::NineSlice { name, x, y, w, h, slice, tex } => match atlas.rect(name) {
                Some(src) => {
                    let tex = match tex {
                        Some((tw, th)) => [*tw as f32, *th as f32],
                        None => [src[2], src[3]],
                    };
                    let dst = [*x as f32, *y as f32, *w as f32, *h as f32];
                    for (s, d) in nine_patch(src, tex, *slice as f32, dst) {
                        host.draw_sprite(s, d);
                    }
                }
                None => host.draw_vanilla_nine_slice(name, *x as f32, *y as f32, *w as f32, *h as f32, 0xffff_ffff),
            },
            UiDraw::Tooltip { x, y, lines } => host.draw_tooltip(lines, *x, *y),
        }
    }
}
