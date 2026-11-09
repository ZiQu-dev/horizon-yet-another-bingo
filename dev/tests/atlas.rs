use horizon_bingo::atlas::{Atlas, PackError, Sprite};

fn solid(name: &str, w: u32, h: u32, px: [u8; 4]) -> Sprite {
    Sprite { name: name.into(), width: w, height: h, rgba: px.iter().copied().cycle().take((w * h * 4) as usize).collect() }
}

fn pixel(a: &Atlas, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * a.width + x) * 4) as usize;
    [a.rgba[i], a.rgba[i + 1], a.rgba[i + 2], a.rgba[i + 3]]
}

#[test]
fn every_sprite_lands_inside_the_atlas_without_touching_another_and_keeps_its_pixels() {
    let sprites = vec![
        solid("red", 122, 132, [255, 0, 0, 255]),
        solid("blue", 122, 132, [0, 0, 255, 255]),
        solid("small", 20, 20, [1, 2, 3, 4]),
        solid("icon", 16, 16, [9, 8, 7, 6]),
    ];
    let atlas = Atlas::pack(&sprites, 1024).unwrap();
    assert_eq!(atlas.len(), 4);
    let rects: Vec<(String, [f32; 4])> = sprites.iter().map(|s| (s.name.clone(), atlas.rect(&s.name).unwrap())).collect();
    for (name, r) in &rects {
        assert!(r[0] + r[2] <= atlas.width as f32 && r[1] + r[3] <= atlas.height as f32, "{name} is outside");
    }
    for (i, (a, ra)) in rects.iter().enumerate() {
        for (b, rb) in &rects[i + 1..] {
            let apart = ra[0] + ra[2] <= rb[0] || rb[0] + rb[2] <= ra[0] || ra[1] + ra[3] <= rb[1] || rb[1] + rb[3] <= ra[1];
            assert!(apart, "{a} and {b} overlap");
        }
    }
    // The pixels at every corner of every sprite are the sprite's own colour.
    for s in &sprites {
        let r = atlas.rect(&s.name).unwrap();
        let want = [s.rgba[0], s.rgba[1], s.rgba[2], s.rgba[3]];
        for (dx, dy) in [(0.0, 0.0), (r[2] - 1.0, 0.0), (0.0, r[3] - 1.0), (r[2] - 1.0, r[3] - 1.0)] {
            assert_eq!(pixel(&atlas, (r[0] + dx) as u32, (r[1] + dy) as u32), want, "{}", s.name);
        }
    }
}

#[test]
fn a_sprite_is_copied_row_by_row_without_shifting() {
    // 3 x 2 with a different value in every pixel.
    let rgba: Vec<u8> = (0..3 * 2 * 4).map(|i| i as u8).collect();
    let s = Sprite { name: "gradient".into(), width: 3, height: 2, rgba: rgba.clone() };
    let atlas = Atlas::pack(&[solid("pad", 5, 5, [200, 200, 200, 200]), s], 64).unwrap();
    let r = atlas.rect("gradient").unwrap();
    let mut got = Vec::new();
    for y in 0..2u32 {
        for x in 0..3u32 {
            got.extend_from_slice(&pixel(&atlas, r[0] as u32 + x, r[1] as u32 + y));
        }
    }
    assert_eq!(got, rgba);
}

#[test]
fn the_nine_team_cards_and_their_shadowed_versions_fit_the_1024_slot() {
    // 18 textures of 122 x 132 (nine colours, plain and shadowed) plus small ones, as the mod ships them.
    let mut sprites: Vec<Sprite> = (0..18).map(|i| solid(&format!("card_{i}"), 122, 132, [i as u8, 0, 0, 255])).collect();
    sprites.extend((0..30).map(|i| solid(&format!("deco_{i}"), 20, 20, [0, i as u8, 0, 255])));
    let atlas = Atlas::pack(&sprites, 1024).unwrap();
    assert!(atlas.width <= 1024 && atlas.height <= 1024);
    assert_eq!(atlas.len(), 48);
}

#[test]
fn bad_input_is_refused() {
    assert_eq!(Atlas::pack(&[solid("huge", 2000, 10, [0; 4])], 1024).unwrap_err(), PackError::TooBig);
    let wrong = Sprite { name: "wrong".into(), width: 2, height: 2, rgba: vec![0; 15] };
    assert_eq!(Atlas::pack(&[wrong], 64).unwrap_err(), PackError::BadSprite("wrong".into()));
    let zero = Sprite { name: "zero".into(), width: 0, height: 4, rgba: vec![] };
    assert_eq!(Atlas::pack(&[zero], 64).unwrap_err(), PackError::BadSprite("zero".into()));
    assert_eq!(Atlas::pack(&[solid("a", 2, 2, [0; 4]), solid("a", 3, 3, [0; 4])], 64).unwrap_err(), PackError::Duplicate("a".into()));
    // Does not fit: two 600-wide sprites stack to 1200 high in a 1024 atlas, three 600 x 600 cannot.
    let big = [solid("a", 600, 600, [0; 4]), solid("b", 600, 600, [0; 4])];
    assert_eq!(Atlas::pack(&big, 1024).unwrap_err(), PackError::TooBig);
    assert!(Atlas::pack(&[], 64).unwrap().is_empty());
}

#[test]
fn the_layout_does_not_depend_on_the_order_the_sprites_are_given_in() {
    let a = vec![solid("x", 30, 30, [1, 1, 1, 1]), solid("y", 20, 40, [2, 2, 2, 2]), solid("z", 10, 10, [3, 3, 3, 3])];
    let mut b = a.clone();
    b.reverse();
    let (pa, pb) = (Atlas::pack(&a, 128).unwrap(), Atlas::pack(&b, 128).unwrap());
    for name in ["x", "y", "z"] {
        assert_eq!(pa.rect(name), pb.rect(name), "{name}");
    }
    assert_eq!(pa.rgba, pb.rgba);
}
