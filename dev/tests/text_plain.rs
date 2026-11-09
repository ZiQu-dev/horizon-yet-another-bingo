
#[test]
fn runs_keep_colours_and_a_components_colour_reaches_its_arguments_unless_they_set_their_own() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::{runs, StyledRun};
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let list = |items: Vec<Nbt>| Nbt::List { elem: 10, items };
    // "[%s] got %s" in gray with a blue first argument and a plain second one
    let text = Text(comp(vec![
        ("translate", s("m.k")),
        ("color", s("gray")),
        ("with", list(vec![comp(vec![("text", s("RefLive")), ("color", s("blue"))]), comp(vec![("text", s("item"))])])),
    ]));
    let lang = |k: &str| (k == "m.k").then(|| "[%s] got %s".to_string());
    let got = runs(&text, &lang);
    let run = |t: &str, c: u32| StyledRun { text: t.into(), rgb: Some(c), head: None };
    assert_eq!(got, vec![run("[", 0xAAAAAA), run("RefLive", 0x5555FF), run("] got item", 0xAAAAAA)]);
    // text under the empty key is plain text too (the 26.x codec writes it so inside a list of components)
    let bare = Text(comp(vec![("", s("RefLive"))]));
    assert_eq!(runs(&bare, &|_| None), vec![StyledRun { text: "RefLive".into(), rgb: None, head: None }]);
}

#[test]
fn a_player_object_is_a_head_two_spaces_wide_and_its_uuid_is_read_from_four_ints() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::{heads, runs, StyledRun};
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let head = comp(vec![("player", comp(vec![("id", Nbt::IntArray(vec![1, 2, 3, -1]))])), ("extra", Nbt::List { elem: 8, items: vec![s(" ")] })]);
    let text = Text(Nbt::List { elem: 10, items: vec![head, comp(vec![("", s("Bob"))])] });
    let uuid = (1u128 << 96) | (2u128 << 64) | (3u128 << 32) | 0xffff_ffff;
    assert_eq!(heads(&text), vec![uuid]);
    assert_eq!(
        runs(&text, &|_| None),
        vec![
            StyledRun { text: "  ".into(), rgb: None, head: Some(uuid) },
            StyledRun { text: " Bob".into(), rgb: None, head: None },
        ]
    );
}

#[test]
fn a_head_inside_a_translation_argument_stays_a_head() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::runs;
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let head = comp(vec![("player", comp(vec![("id", Nbt::IntArray(vec![0, 0, 0, 9]))]))]);
    let arg = comp(vec![("text", s("")), ("extra", Nbt::List { elem: 10, items: vec![head, comp(vec![("", s("Bob"))])] })]);
    let text = Text(comp(vec![("translate", s("m.k")), ("with", Nbt::List { elem: 10, items: vec![arg] })]));
    let lang = |k: &str| (k == "m.k").then(|| "%s got it".to_string());
    let got = runs(&text, &lang);
    assert_eq!(got.iter().map(|r| (r.text.as_str(), r.head)).collect::<Vec<_>>(), vec![("  ", Some(9)), ("Bob got it", None)]);
}

#[test]
fn a_hostile_nest_of_repeated_placeholders_stays_within_the_budget_and_returns_quickly() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::flatten;
    use std::time::{Duration, Instant};
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let nest = |leaf: &str| {
        let mut node = comp(vec![("text", s(leaf))]);
        for _ in 0..12 {
            node = comp(vec![("translate", s("m.k")), ("with", Nbt::List { elem: 10, items: vec![node] })]);
        }
        node
    };
    let template = "%1$s".repeat(10);
    let lang = |k: &str| (k == "m.k").then(|| template.clone());

    // Ten copies per level, twelve levels: 10^12 characters if nothing stopped it.
    let started = Instant::now();
    let out = flatten(&Text(nest("x")), &lang);
    assert!(started.elapsed() < Duration::from_secs(1), "took {:?}", started.elapsed());
    assert!(!out.is_empty() && out.chars().count() <= 8192, "{} chars", out.chars().count());
    assert!(out.chars().all(|c| c == 'x'));

    // The same nest around an empty leaf produces nothing, and must still finish quickly.
    let started = Instant::now();
    let out = flatten(&Text(nest("")), &lang);
    assert!(started.elapsed() < Duration::from_secs(1), "took {:?}", started.elapsed());
    assert_eq!(out, "");
}

#[test]
fn an_extra_nested_forty_deep_does_not_overflow_and_its_shallow_part_is_shown() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::flatten;
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let mut node = comp(vec![("text", s("L"))]);
    for _ in 0..40 {
        node = comp(vec![("text", s("L")), ("extra", Nbt::List { elem: 10, items: vec![node] })]);
    }
    let out = flatten(&Text(node), &|_| None);
    assert!(out.chars().all(|c| c == 'L'));
    // Levels 0 to 32 are shown; the rest are not.
    assert!((3..=33).contains(&out.chars().count()), "{out:?}");
}

#[test]
fn a_template_expands_at_most_sixty_four_placeholders() {
    use horizon_bingo::nbt::Nbt;
    use horizon_bingo::text::Text;
    use horizon_bingo::text_plain::flatten;
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let args: Vec<Nbt> = (0..100).map(|_| comp(vec![("text", s("a"))])).collect();
    let text = Text(comp(vec![("translate", s("m.k")), ("with", Nbt::List { elem: 10, items: args })]));
    let template = "%s".repeat(100);
    let lang = |k: &str| (k == "m.k").then(|| template.clone());
    assert_eq!(flatten(&text, &lang), "a".repeat(64));
}
