//! Item ids read from a name list: the line number is the id, blank lines keep the numbering, and duplicates keep
//! their first line. Expected values are the line numbers counted by hand.
use std::path::Path;

use horizon_bingo::server::items::ItemIds;

#[test]
fn the_line_number_is_the_id() {
    let ids = ItemIds::from_names("minecraft:stone\nminecraft:dirt\nminecraft:grass_block\n");
    assert_eq!(ids.id("minecraft:stone"), Some(0));
    assert_eq!(ids.id("minecraft:dirt"), Some(1));
    assert_eq!(ids.id("minecraft:grass_block"), Some(2));
    assert_eq!(ids.len(), 3);
    assert!(!ids.is_empty());
}

#[test]
fn blank_lines_keep_their_number_but_add_no_item() {
    let ids = ItemIds::from_names("a\n\n\nb\n");
    assert_eq!(ids.id("a"), Some(0));
    assert_eq!(ids.id("b"), Some(3));
    assert_eq!(ids.len(), 2);
}

#[test]
fn surrounding_spaces_and_carriage_returns_are_not_part_of_the_name() {
    let ids = ItemIds::from_names("  minecraft:bone \r\nminecraft:string\r\n");
    assert_eq!(ids.id("minecraft:bone"), Some(0));
    assert_eq!(ids.id("minecraft:string"), Some(1));
    assert_eq!(ids.id("  minecraft:bone "), None);
}

#[test]
fn a_name_listed_twice_keeps_its_first_number() {
    let ids = ItemIds::from_names("a\nb\na\n");
    assert_eq!(ids.id("a"), Some(0));
    assert_eq!(ids.id("b"), Some(1));
    assert_eq!(ids.len(), 2);
}

#[test]
fn unknown_names_and_empty_input_have_no_id() {
    let ids = ItemIds::from_names("a\n");
    assert_eq!(ids.id("missing"), None);
    assert_eq!(ids.id(""), None);

    let empty = ItemIds::from_names("");
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.id("a"), None);
}

#[test]
fn load_reads_the_file_and_reports_a_missing_one() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("server_items_names.txt");
    std::fs::write(&path, "x\n\ny\n").unwrap();
    let ids = ItemIds::load(&path).expect("the file exists");
    assert_eq!(ids.id("x"), Some(0));
    assert_eq!(ids.id("y"), Some(2));

    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("server_items_no_such_file.txt");
    let error = ItemIds::load(&missing).err().expect("the file does not exist");
    assert!(error.contains("server_items_no_such_file.txt"), "the error names the file: {error}");
}

#[test]
fn the_missing_names_warning_names_the_path_the_reason_and_the_lost_icons() {
    use horizon_bingo::server::items::missing_names_warning;
    let text = missing_names_warning(
        Path::new("/srv/game/horizon-mods/.horizon-mod-data/bingo_native/assets/item_names.txt"),
        "No such file or directory (os error 2)",
    );
    assert!(text.starts_with("item_names.txt not loaded from /srv/game/horizon-mods/.horizon-mod-data/bingo_native/assets/item_names.txt"));
    assert!(text.contains("(No such file or directory (os error 2))"), "{text}");
    assert!(text.contains("will have no item icons"), "{text}");
    assert!(text.contains("BINGO_ASSETS"), "the warning says how to fix it: {text}");
}
