//! The unified draw list: nine-slice splitting, the conversions from the four screens' commands, and `run` through a
//! recording host. Every expected number is worked out by hand in the comment beside it.
use horizon_bingo::atlas::{Atlas, Sprite};
use horizon_bingo::card_draw::Draw;
use horizon_bingo::host::{scale_alpha, Argb, Host};
use horizon_bingo::screen_cards::ScreenDraw as CardsDraw;
use horizon_bingo::screen_controller::{outside, run_faded};
use horizon_bingo::screen_end::{ScreenDraw as EndDraw, TooltipLine};
use horizon_bingo::screen_main::ScreenDraw as MainDraw;
use horizon_bingo::screen_placement::PlacementDraw;
use horizon_bingo::state::SoundKey;
use horizon_bingo::ui_draw::{nine_slice, run, UiDraw};

#[derive(Default)]
struct Recorder {
    sprites: Vec<([f32; 4], [f32; 4])>,
    fills: Vec<(i32, i32, i32, i32, Argb)>,
    texts: Vec<(i32, i32, String, Argb, bool)>,
    items: Vec<(String, f32, f32, f32)>,
    vanilla: Vec<(String, f32, f32, f32, f32, Argb)>,
    tooltips: Vec<(Vec<String>, i32, i32)>,
    /// `fills.len()` when `begin_alpha` was called, and (`fills.len()`, alpha) for each `end_alpha`.
    fade_begins: Vec<usize>,
    fade_ends: Vec<(usize, f32)>,
}

impl Host for Recorder {
    fn begin_alpha(&mut self) -> usize {
        self.fade_begins.push(self.fills.len());
        self.fills.len()
    }
    fn end_alpha(&mut self, _start: usize, alpha: f32) {
        self.fade_ends.push((self.fills.len(), alpha));
    }
    fn gui_size(&self) -> Option<(i32, i32)> {
        None
    }
    fn cursor(&self) -> (i32, i32) {
        (0, 0)
    }
    fn is_playing_screen(&self) -> bool {
        false
    }
    fn key_down(&self, _: &str) -> bool {
        false
    }
    fn open_screen(&mut self) -> bool {
        false
    }
    fn close_screen(&mut self) {}
    fn screen_is_open(&self) -> bool {
        false
    }
    fn send_to_server(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb) {
        self.fills.push((x, y, w, h, color));
    }
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool) {
        self.texts.push((x, y, text.to_string(), color, shadow));
    }
    fn text_width(&self, text: &str) -> i32 {
        6 * text.chars().count() as i32
    }
    fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> bool {
        true
    }
    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]) {
        self.sprites.push((src, dst));
    }
    fn draw_vanilla_nine_slice(&mut self, sprite: &str, x: f32, y: f32, w: f32, h: f32, argb: Argb) {
        self.vanilla.push((sprite.to_string(), x, y, w, h, argb));
    }
    fn draw_tooltip(&mut self, lines: &[String], x: i32, y: i32) {
        self.tooltips.push((lines.to_vec(), x, y));
    }
    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32) {
        self.items.push((name.to_string(), x, y, size));
    }
    fn gui_width(&self) -> Option<i32> {
        None
    }
    fn play_sound(&mut self, _: SoundKey, _: f32, _: f32) {}
}

/// `panel` is 20 x 20 and taller, so it is packed first at (0, 0); `pad` is 4 x 4 and lands right of it at (20, 0).
fn atlas() -> Atlas {
    let sprite = |name: &str, w: u32, h: u32| Sprite {
        name: name.into(),
        width: w,
        height: h,
        rgba: vec![0; (w * h * 4) as usize],
    };
    Atlas::pack(&[sprite("panel", 20, 20), sprite("pad", 4, 4)], 256).unwrap()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn near4(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| near(*x, *y))
}

// ---- nine_slice ------------------------------------------------------------------------------------------------

#[test]
fn a_40_by_30_box_from_a_20_by_20_texture_with_slice_4_has_nine_pieces_worked_by_hand() {
    // Borders 4, 16, 20 on x and y in the texture, so the edges are 4 wide and the middle 12.
    // Destination: columns 0..4, 4..36, 36..40; rows 0..4, 4..26, 26..30.
    let pieces = nine_slice([0.0, 0.0, 20.0, 20.0], [20.0, 20.0], 4.0, [0.0, 0.0, 40.0, 30.0]);
    let want = [
        ([0.0, 0.0, 4.0, 4.0], [0.0, 0.0, 4.0, 4.0]),
        ([4.0, 0.0, 12.0, 4.0], [4.0, 0.0, 32.0, 4.0]),
        ([16.0, 0.0, 4.0, 4.0], [36.0, 0.0, 4.0, 4.0]),
        ([0.0, 4.0, 4.0, 12.0], [0.0, 4.0, 4.0, 22.0]),
        ([4.0, 4.0, 12.0, 12.0], [4.0, 4.0, 32.0, 22.0]),
        ([16.0, 4.0, 4.0, 12.0], [36.0, 4.0, 4.0, 22.0]),
        ([0.0, 16.0, 4.0, 4.0], [0.0, 26.0, 4.0, 4.0]),
        ([4.0, 16.0, 12.0, 4.0], [4.0, 26.0, 32.0, 4.0]),
        ([16.0, 16.0, 4.0, 4.0], [36.0, 26.0, 4.0, 4.0]),
    ];
    assert_eq!(pieces.len(), 9);
    for (got, want) in pieces.iter().zip(want.iter()) {
        assert!(near4(got.0, want.0), "src {:?} != {:?}", got.0, want.0);
        assert!(near4(got.1, want.1), "dst {:?} != {:?}", got.1, want.1);
    }
}

#[test]
fn a_box_smaller_than_twice_the_slice_shrinks_its_borders_and_drops_the_empty_centre() {
    // Destination 6 x 20: borders min(4, 3) = 3 on x, 4 on y. Columns 0..3, 3..3 (empty), 3..6; the centre is gone.
    // Rows 0..4, 4..16, 16..20.
    let pieces = nine_slice([0.0, 0.0, 20.0, 20.0], [20.0, 20.0], 4.0, [0.0, 0.0, 6.0, 20.0]);
    let dst: Vec<[f32; 4]> = pieces.iter().map(|p| p.1).collect();
    assert_eq!(
        dst,
        vec![
            [0.0, 0.0, 3.0, 4.0],
            [3.0, 0.0, 3.0, 4.0],
            [0.0, 4.0, 3.0, 12.0],
            [3.0, 4.0, 3.0, 12.0],
            [0.0, 16.0, 3.0, 4.0],
            [3.0, 16.0, 3.0, 4.0],
        ]
    );
}

#[test]
fn a_box_of_exactly_twice_the_slice_keeps_four_corners_and_no_edges() {
    // 8 x 8 box, slice 4: borders 4 and 4, so each corner is 4 x 4 and nothing is left for the edges or the centre.
    let pieces = nine_slice([0.0, 0.0, 20.0, 20.0], [20.0, 20.0], 4.0, [0.0, 0.0, 8.0, 8.0]);
    assert_eq!(pieces.len(), 4);
    let src: Vec<[f32; 4]> = pieces.iter().map(|p| p.0).collect();
    assert_eq!(src, vec![[0.0, 0.0, 4.0, 4.0], [16.0, 0.0, 4.0, 4.0], [0.0, 16.0, 4.0, 4.0], [16.0, 16.0, 4.0, 4.0]]);
}

#[test]
fn a_texture_in_a_larger_atlas_is_read_through_its_scale() {
    // The texture sits at (10, 20) in an atlas where it is 40 x 40: each texel is 2 atlas pixels.
    // The top edge's source starts after the 4-texel corner: x = 10 + 4 * 2 = 18, width 12 * 2 = 24, height 4 * 2 = 8.
    let pieces = nine_slice([10.0, 20.0, 40.0, 40.0], [20.0, 20.0], 4.0, [0.0, 0.0, 40.0, 30.0]);
    assert!(near4(pieces[0].0, [10.0, 20.0, 8.0, 8.0]));
    assert!(near4(pieces[1].0, [18.0, 20.0, 24.0, 8.0]));
    assert!(near4(pieces[1].1, [4.0, 0.0, 32.0, 4.0]));
}

#[test]
fn the_destination_offset_moves_every_piece_and_the_source_is_not_moved() {
    // Box at (100, 50): the top left corner is at (100, 50), the centre starts at (104, 54).
    let pieces = nine_slice([0.0, 0.0, 20.0, 20.0], [20.0, 20.0], 4.0, [100.0, 50.0, 40.0, 30.0]);
    assert!(near4(pieces[0].1, [100.0, 50.0, 4.0, 4.0]));
    assert!(near4(pieces[4].1, [104.0, 54.0, 32.0, 22.0]));
    assert!(near4(pieces[4].0, [4.0, 4.0, 12.0, 12.0]));
}

#[test]
fn an_empty_box_draws_no_pieces() {
    assert!(nine_slice([0.0, 0.0, 20.0, 20.0], [20.0, 20.0], 4.0, [0.0, 0.0, 0.0, 30.0]).is_empty());
}

// ---- conversions -----------------------------------------------------------------------------------------------

#[test]
fn a_label_is_placed_at_anchor_plus_the_scaled_offset_turned_by_its_rotation() {
    // Anchor (100, 50), offset (10, 0), scale 1, quarter turn: (10, 0) turns to (0, 10), so the text lands at (100, 60).
    let label = MainDraw::Label {
        text: "t".into(),
        anchor: (100.0, 50.0),
        offset: (10.0, 0.0),
        scale: 1.0,
        rotation: std::f32::consts::FRAC_PI_2,
        argb: 0xffff_ffff,
        shadow: true,
    };
    // The frame goes to the host as it is, so the glyphs turn and scale with it...
    assert_eq!(
        UiDraw::from(&label),
        UiDraw::TextXf {
            text: "t".into(),
            anchor: (100.0, 50.0),
            offset: (10.0, 0.0),
            scale: 1.0,
            rotation: std::f32::consts::FRAC_PI_2,
            argb: 0xffff_ffff,
            shadow: true
        }
    );
    // ...and a host that only has upright text puts the corner where the frame does.
    let mut host = Recorder::default();
    run(&[UiDraw::from(&label)], &atlas(), &mut host);
    assert_eq!(host.texts, vec![(100, 60, "t".to_string(), 0xffff_ffff, true)]);
}

#[test]
fn a_label_offset_is_scaled_before_it_is_turned() {
    // Offset (-17, 0) at scale 2 is (-34, 0); no turn, so the text is at (96.8 - 34, 123) = (62.8, 123).
    let label = MainDraw::Label {
        text: "t".into(),
        anchor: (96.8, 123.0),
        offset: (-17.0, 0.0),
        scale: 2.0,
        rotation: 0.0,
        argb: 0,
        shadow: false,
    };
    let mut host = Recorder::default();
    run(&[UiDraw::from(&label)], &atlas(), &mut host);
    assert_eq!(host.texts, vec![(63, 123, "t".to_string(), 0, false)]);
}

#[test]
fn a_rotated_square_keeps_its_turn_for_the_host_and_falls_back_to_a_fill_of_its_square() {
    // Centre (100, 50), half side 16: the square spans x 84..116 and y 34..66.
    let sq = MainDraw::Square { cx: 100.0, cy: 50.0, half: 16.0, rotation: 0.3, argb: 0xff00_ff00 };
    assert_eq!(UiDraw::from(&sq), UiDraw::SquareXf { cx: 100.0, cy: 50.0, half: 16.0, rotation: 0.3, argb: 0xff00_ff00 });
    let mut host = Recorder::default();
    run(&[UiDraw::from(&sq)], &atlas(), &mut host);
    assert_eq!(host.fills, vec![(84, 34, 32, 32, 0xff00_ff00)]);
}

#[test]
fn main_nine_patch_keeps_its_texture_size_and_end_nine_slice_too() {
    let main = MainDraw::NinePatch { name: "bingo/a".into(), x: 1, y: 2, w: 30, h: 40, slice: 4, texture_w: 16, texture_h: 24 };
    assert_eq!(
        UiDraw::from(&main),
        UiDraw::NineSlice { name: "bingo/a".into(), x: 1, y: 2, w: 30, h: 40, slice: 4, tex: Some((16, 24)) }
    );
    let end = EndDraw::NineSlice { name: "bingo/b".into(), x: 0, y: 0, w: 9, h: 9, slice: 2, tex_w: 8, tex_h: 8 };
    assert_eq!(
        UiDraw::from(&end),
        UiDraw::NineSlice { name: "bingo/b".into(), x: 0, y: 0, w: 9, h: 9, slice: 2, tex: Some((8, 8)) }
    );
}

#[test]
fn the_cards_nine_patch_has_no_texture_size_so_the_atlas_decides_it() {
    let cards = CardsDraw::NinePatch { name: "bingo/game_container".into(), x: 5, y: 6, w: 100, h: 50, slice: 4 };
    assert_eq!(
        UiDraw::from(&cards),
        UiDraw::NineSlice { name: "bingo/game_container".into(), x: 5, y: 6, w: 100, h: 50, slice: 4, tex: None }
    );
}

#[test]
fn a_tooltip_keeps_the_text_of_each_line_in_order() {
    let end = EndDraw::Tooltip {
        x: 7,
        y: 8,
        lines: vec![
            TooltipLine { head: None, text: "Team".into() },
            TooltipLine { head: Some(1), text: "Alex".into() },
        ],
    };
    assert_eq!(
        UiDraw::from(&end),
        UiDraw::Tooltip { x: 7, y: 8, lines: vec!["Team".into(), "Alex".into()] }
    );
}

#[test]
fn a_scaled_fill_keeps_its_fraction_and_a_card_draw_keeps_its_whole_numbers() {
    let fill = PlacementDraw::ScaledFill { x: 10.5, y: 2.25, w: 3.0, h: 4.0, argb: 1 };
    assert_eq!(UiDraw::from(&fill), UiDraw::Fill { x: 10.5, y: 2.25, w: 3.0, h: 4.0, argb: 1 });
    let card = CardsDraw::Plain(Draw::Text { x: 12, y: 10, text: "x".into(), argb: 9 });
    assert_eq!(
        UiDraw::from(&card),
        UiDraw::Text { x: 12.0, y: 10.0, text: "x".into(), argb: 9, shadow: false, obfuscated: false }
    );
    let placed = PlacementDraw::Draw(Draw::Item { name: "minecraft:stone".into(), x: 1.0, y: 2.0, size: 16.0 });
    assert_eq!(
        UiDraw::from(&placed),
        UiDraw::Item { name: "minecraft:stone".into(), x: 1.0, y: 2.0, size: 16.0 }
    );
}

// ---- run -------------------------------------------------------------------------------------------------------

#[test]
fn a_sprite_is_drawn_from_its_atlas_rectangle_and_a_missing_one_is_skipped() {
    let atlas = atlas();
    let cmds = vec![
        UiDraw::Sprite { name: "pad".into(), x: 3.0, y: 4.0, w: 8.0, h: 8.0 },
        UiDraw::Sprite { name: "absent".into(), x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
    ];
    let mut host = Recorder::default();
    run(&cmds, &atlas, &mut host);
    // `pad` is packed at (20, 0), 4 x 4.
    assert_eq!(host.sprites, vec![([20.0, 0.0, 4.0, 4.0], [3.0, 4.0, 8.0, 8.0])]);
}

#[test]
fn a_fill_is_drawn_on_whole_pixels_from_its_rounded_edges() {
    // x 10.4 -> 10, right edge 15.4 -> 15: width 5. y 20.6 -> 21, bottom 23.6 -> 24: height 3.
    let cmds = vec![UiDraw::Fill { x: 10.4, y: 20.6, w: 5.0, h: 3.0, argb: 0xff00_0000 }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert_eq!(host.fills, vec![(10, 21, 5, 3, 0xff00_0000)]);
}

#[test]
fn a_fill_with_no_area_draws_nothing() {
    let cmds = vec![UiDraw::Fill { x: 4.0, y: 4.0, w: 0.0, h: 9.0, argb: 1 }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert!(host.fills.is_empty());
}

#[test]
fn text_is_drawn_at_its_rounded_position_with_its_shadow() {
    // 3.5 rounds up to 4 and 4.0 stays 4.
    let cmds = vec![UiDraw::Text { x: 3.5, y: 4.0, text: "Bingo".into(), argb: 0xffff_ffff, shadow: true, obfuscated: false }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert_eq!(host.texts, vec![(4, 4, "Bingo".to_string(), 0xffff_ffff, true)]);
}

#[test]
fn an_item_is_passed_to_the_host_by_name() {
    let cmds = vec![UiDraw::Item { name: "minecraft:stone".into(), x: 1.0, y: 2.0, size: 16.0 }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert_eq!(host.items, vec![("minecraft:stone".to_string(), 1.0, 2.0, 16.0)]);
}

#[test]
fn a_nine_slice_in_the_atlas_is_tiled_like_the_originals_draw_nine_patch() {
    // `panel` is 20 x 20 at (0, 0), its centre 12 wide; drawn into 40 x 30 with slice 4 the centre is 32 x 22: 3 x 2
    // centre pieces, 3 x 2 horizontal and 2 x 2 vertical edge pieces, 4 corners.
    let cmds = vec![UiDraw::NineSlice { name: "panel".into(), x: 0, y: 0, w: 40, h: 30, slice: 4, tex: None }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert_eq!(host.sprites.len(), 20);
    assert_eq!(host.sprites[0], ([4.0, 4.0, 12.0, 12.0], [4.0, 4.0, 12.0, 12.0]));
    assert_eq!(host.sprites[5], ([4.0, 4.0, 8.0, 10.0], [28.0, 16.0, 8.0, 10.0]));
    assert!(host.vanilla.is_empty());
}

#[test]
fn a_nine_slice_outside_the_atlas_falls_back_to_the_vanilla_widget() {
    let cmds = vec![UiDraw::NineSlice { name: "widget/button".into(), x: 2, y: 3, w: 40, h: 30, slice: 4, tex: Some((20, 20)) }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert!(host.sprites.is_empty());
    assert_eq!(host.vanilla, vec![("widget/button".to_string(), 2.0, 3.0, 40.0, 30.0, 0xffff_ffff)]);
}

#[test]
fn a_tooltip_is_passed_with_its_lines_and_corner() {
    let cmds = vec![UiDraw::Tooltip { x: 7, y: 8, lines: vec!["a".into(), "b".into()] }];
    let mut host = Recorder::default();
    run(&cmds, &atlas(), &mut host);
    assert_eq!(host.tooltips, vec![(vec!["a".to_string(), "b".to_string()], 7, 8)]);
}

#[test]
fn a_whole_screen_runs_in_order_through_the_host() {
    // A rotated-square fill, a label and a tooltip: three host calls, in the order of the list.
    let main = vec![
        UiDraw::from(&MainDraw::Square { cx: 10.0, cy: 10.0, half: 2.0, rotation: 1.0, argb: 5 }),
        UiDraw::from(&MainDraw::Label {
            text: "L".into(),
            anchor: (0.0, 0.0),
            offset: (1.0, 1.0),
            scale: 1.0,
            rotation: 0.0,
            argb: 6,
            shadow: false,
        }),
        UiDraw::Tooltip { x: 0, y: 0, lines: vec!["T".into()] },
    ];
    let mut host = Recorder::default();
    run(&main, &atlas(), &mut host);
    assert_eq!(host.fills, vec![(8, 8, 4, 4, 5)]);
    assert_eq!(host.texts, vec![(1, 1, "L".to_string(), 6, false)]);
    assert_eq!(host.tooltips, vec![(vec!["T".to_string()], 0, 0)]);
}

#[test]
fn a_nine_patch_tiles_its_centre_and_edges_in_pieces_of_the_textures_own_centre_size() {
    // IDrawService.drawNinePatch: a 128 px texture cut at 32 has a 64 px centre; 200 x 300 needs 3 x 4 centre pieces.
    let pieces = horizon_bingo::ui_draw::nine_patch([0.0, 0.0, 128.0, 128.0], [128.0, 128.0], 32.0, [0.0, 0.0, 200.0, 300.0]);
    // 12 centre pieces, 3 x 2 horizontal edges, 4 x 2 vertical edges, 4 corners
    assert_eq!(pieces.len(), 30);
    // the first centre piece is the whole texture centre at the box's inner corner
    assert_eq!(pieces[0], ([32.0, 32.0, 64.0, 64.0], [32.0, 32.0, 64.0, 64.0]));
    // the last column and row are cut short: 200 - 64 = 136 = 64 + 64 + 8 wide, 300 - 64 = 236 = 3 * 64 + 44 high
    let last = pieces[11];
    assert_eq!(last, ([32.0, 32.0, 8.0, 44.0], [160.0, 224.0, 8.0, 44.0]));
    // corners come last: the bottom right takes the texture's bottom right
    assert_eq!(pieces[29], ([96.0, 96.0, 32.0, 32.0], [168.0, 268.0, 32.0, 32.0]));
    // the atlas holds the sprite at an offset: source rectangles move with it
    let moved = horizon_bingo::ui_draw::nine_patch([10.0, 20.0, 128.0, 128.0], [128.0, 128.0], 32.0, [0.0, 0.0, 200.0, 300.0]);
    assert_eq!(moved[0].0, [42.0, 52.0, 64.0, 64.0]);
}

#[test]
fn a_scaled_title_is_scaled_about_the_origin_so_its_halved_position_doubles() {
    // The history draws the title in the frame of `scale(2)` at (7, 12): the glyphs then land at (14, 24) at twice the size.
    let draw = CardsDraw::ScaledText { x: 7.0, y: 12.0, scale: 2.0, text: "Winner".into(), argb: 0xffff_ffff };
    assert_eq!(
        UiDraw::from(&draw),
        UiDraw::TextXf {
            text: "Winner".into(),
            anchor: (0.0, 0.0),
            offset: (7.0, 12.0),
            scale: 2.0,
            rotation: 0.0,
            argb: 0xffff_ffff,
            shadow: false
        }
    );
}

#[test]
fn an_end_screen_sprite_is_a_sprite_command_drawn_from_the_atlas_at_its_place() {
    // A face is 8 x 8 at (40, 21), named like any atlas sprite: `pad` is packed at (20, 0), 4 x 4.
    let end = EndDraw::Sprite { name: "pad".into(), x: 40, y: 21, w: 8, h: 8 };
    assert_eq!(UiDraw::from(&end), UiDraw::Sprite { name: "pad".into(), x: 40.0, y: 21.0, w: 8.0, h: 8.0 });
    let mut host = Recorder::default();
    run(&[UiDraw::from(&end)], &atlas(), &mut host);
    assert_eq!(host.sprites, vec![([20.0, 0.0, 4.0, 4.0], [40.0, 21.0, 8.0, 8.0])]);
}

// ---- fading ----------------------------------------------------------------------------------------------------------

#[test]
fn scale_alpha_multiplies_the_top_byte_and_keeps_the_colour_worked_by_hand() {
    // 0xff * 0.5 = 127.5, rounded half up to 128 = 0x80.
    assert_eq!(scale_alpha(0xff33_6699, 0.5), 0x8033_6699);
    // 0x80 * 0.5 = 64 exactly.
    assert_eq!(scale_alpha(0x80ff_8000, 0.5), 0x40ff_8000);
    // 0x12 * 0.5 = 9: the colour bytes 0x34, 0x56, 0x78 stay as they are.
    assert_eq!(scale_alpha(0x1234_5678, 0.5), 0x0934_5678);
    // 0xff * 0.25 = 63.75, rounded to 64 = 0x40.
    assert_eq!(scale_alpha(0xff33_6699, 0.25), 0x4033_6699);
    // 0.0 makes it clear, 1.0 leaves it alone, and values outside 0..=1 are clamped.
    assert_eq!(scale_alpha(0xff33_6699, 0.0), 0x0033_6699);
    assert_eq!(scale_alpha(0xff33_6699, 1.0), 0xff33_6699);
    assert_eq!(scale_alpha(0xff33_6699, 2.0), 0xff33_6699);
    assert_eq!(scale_alpha(0xff33_6699, -1.0), 0x0033_6699);
}

#[test]
fn the_alpha_an_exp_out_fade_ends_at_changes_no_byte() {
    // ExpOut stops at 1 - 2^-10 = 0.9990234375, not at 1, so the cards keep being passed that value. A byte a becomes
    // a - a / 1024, at most 0.25 less, which rounds back to a.
    for a in 0u32..=255 {
        assert_eq!(scale_alpha((a << 24) | 0x00ab_cdef, 0.999_023_4), (a << 24) | 0x00ab_cdef, "alpha {a}");
    }
}

#[test]
fn run_faded_begins_after_the_draws_before_the_span_and_ends_after_its_last_draw() {
    // Three fills; the cards are the middle one. The fade starts after one fill and ends after two.
    let fill = |argb: u32| UiDraw::Fill { x: 0.0, y: 0.0, w: 4.0, h: 4.0, argb };
    let cmds = [fill(0xff00_0001), fill(0xff00_0002), fill(0xff00_0003)];
    let mut host = Recorder::default();
    run_faded(&cmds, &[(1, 2)], 0.5, &atlas(), &mut host);
    assert_eq!(host.fade_begins, vec![1]);
    assert_eq!(host.fade_ends, vec![(2, 0.5)]);
    assert_eq!(host.fills.iter().map(|f| f.4).collect::<Vec<_>>(), vec![0xff00_0001, 0xff00_0002, 0xff00_0003]);
}

#[test]
fn run_faded_at_full_opacity_calls_neither_begin_nor_end() {
    let fill = |argb: u32| UiDraw::Fill { x: 0.0, y: 0.0, w: 4.0, h: 4.0, argb };
    let cmds = [fill(0xff00_0001), fill(0xff00_0002), fill(0xff00_0003)];
    let mut host = Recorder::default();
    run_faded(&cmds, &[(1, 2)], 1.0, &atlas(), &mut host);
    assert!(host.fade_begins.is_empty() && host.fade_ends.is_empty());
    assert_eq!(host.fills.iter().map(|f| f.4).collect::<Vec<_>>(), vec![0xff00_0001, 0xff00_0002, 0xff00_0003]);
}

#[test]
fn run_faded_passes_the_opacity_through_for_a_span_covering_the_whole_list() {
    let fill = |argb: u32| UiDraw::Fill { x: 0.0, y: 0.0, w: 4.0, h: 4.0, argb };
    let cmds = [fill(0xff00_0001), fill(0xff00_0002), fill(0xff00_0003)];
    let mut host = Recorder::default();
    run_faded(&cmds, &[(0, 3)], 0.25, &atlas(), &mut host);
    assert_eq!(host.fade_begins, vec![0]);
    assert_eq!(host.fade_ends, vec![(3, 0.25)]);
}

#[test]
fn run_faded_fades_each_span_and_runs_the_gaps_between_them_unfaded() {
    // Five fills, spans 0..1 and 3..4. The first fade begins before fill 0 and ends after it (one fill drawn). The
    // second begins after fills 1 and 2 (three drawn) and ends after fill 3. Fill 4 runs after the last span.
    let fill = |argb: u32| UiDraw::Fill { x: 0.0, y: 0.0, w: 4.0, h: 4.0, argb };
    let cmds = [fill(1), fill(2), fill(3), fill(4), fill(5)];
    let mut host = Recorder::default();
    run_faded(&cmds, &[(0, 1), (3, 4)], 0.5, &atlas(), &mut host);
    assert_eq!(host.fade_begins, vec![0, 3]);
    assert_eq!(host.fade_ends, vec![(1, 0.5), (4, 0.5)]);
    assert_eq!(host.fills.iter().map(|f| f.4).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn outside_leaves_the_parts_of_a_span_that_the_entrance_card_does_not_cover() {
    // The span 2..9 holds the entrance card's draws at 3..5 and its label at 6..7: what is left is 2..3, 5..6 and 7..9.
    assert_eq!(outside((2, 9), &[(3, 5), (6, 7)]), vec![(2, 3), (5, 6), (7, 9)]);
    // A card at either edge leaves no empty piece behind.
    assert_eq!(outside((2, 9), &[(2, 4), (7, 9)]), vec![(4, 7)]);
    // With no entrance card the whole span is faded, and when the card covers all of it nothing is.
    assert_eq!(outside((2, 9), &[]), vec![(2, 9)]);
    // An empty hole (a card with no label, say) cuts nothing, so the fade is not split for it.
    assert_eq!(outside((2, 9), &[(5, 5)]), vec![(2, 9)]);
    assert_eq!(outside((2, 9), &[(2, 9)]), Vec::<(usize, usize)>::new());
}

#[test]
fn run_faded_stops_once_no_alpha_byte_would_change_and_not_before() {
    // 255 * 0.998 = 254.49 rounds to 254, so the 0xff alpha of a vertex does change: the span is faded.
    assert_eq!(scale_alpha(0xff00_0001, 0.998), 0xfe00_0001);
    let fill = UiDraw::Fill { x: 0.0, y: 0.0, w: 4.0, h: 4.0, argb: 0xff00_0001 };
    let mut host = Recorder::default();
    run_faded(&[fill.clone()], &[(0, 1)], 0.998, &atlas(), &mut host);
    assert_eq!(host.fade_begins, vec![0]);
    assert_eq!(host.fade_ends, vec![(1, 0.998)]);
    // 255 * 0.9981 = 254.5155 rounds to 255, and every byte below it rounds back to itself: no pass at all.
    for a in 0u32..=255 {
        assert_eq!(scale_alpha((a << 24) | 1, 0.9981), (a << 24) | 1, "alpha {a}");
    }
    let mut host = Recorder::default();
    run_faded(&[fill], &[(0, 1)], 0.9981, &atlas(), &mut host);
    assert!(host.fade_begins.is_empty() && host.fade_ends.is_empty());
}
