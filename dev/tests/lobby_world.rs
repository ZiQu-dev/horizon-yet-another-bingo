//! The lobby world's install: `saves/<folder>` is made from the template once, whole, and never over a save.
use horizon_bingo::lobby_world::{install, Install};
use std::fs;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bingo-lobby-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn template(root: &PathBuf) -> PathBuf {
    let t = root.join("lobby_world");
    fs::create_dir_all(t.join("dimensions/bingo/lobby/region")).unwrap();
    fs::write(t.join("level.dat"), b"dat").unwrap();
    fs::write(t.join("dimensions/bingo/lobby/region/r.0.0.mca"), b"region").unwrap();
    fs::write(t.join("session.lock"), b"x").unwrap();
    t
}

#[test]
fn the_save_is_made_from_the_template_once_and_never_over_a_save() {
    let root = scratch("make");
    let t = template(&root);
    let saves = root.join("saves");
    assert_eq!(install(&t, &saves, "bingo"), Ok(Install::Created));
    assert_eq!(fs::read(saves.join("bingo/level.dat")).unwrap(), b"dat");
    assert_eq!(fs::read(saves.join("bingo/dimensions/bingo/lobby/region/r.0.0.mca")).unwrap(), b"region");
    assert!(!saves.join("bingo/session.lock").exists(), "the lock is not copied");
    assert!(fs::read_dir(&saves).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')), "no temporary folder is left");
    // A second press finds the save and leaves it as the player has it.
    fs::write(saves.join("bingo/level.dat"), b"played").unwrap();
    assert_eq!(install(&t, &saves, "bingo"), Ok(Install::Exists));
    assert_eq!(fs::read(saves.join("bingo/level.dat")).unwrap(), b"played");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_copy_loses_the_lobby_dimension_file_and_keeps_the_rest_of_the_datapack() {
    let root = scratch("dimension");
    let t = root.join("lobby_world");
    let datapack = t.join("datapacks/bingo");
    for file in [
        "data/bingo/dimension/lobby.json",
        "data/bingo/dimension_type/lobby.json",
        "data/bingo/worldgen/biome/lobby.json",
        "pack.mcmeta",
    ] {
        fs::create_dir_all(datapack.join(file).parent().unwrap()).unwrap();
        fs::write(datapack.join(file), format!("content of {file}")).unwrap();
    }
    fs::create_dir_all(t.join("dimensions/bingo/lobby/region")).unwrap();
    fs::write(t.join("level.dat"), b"dat").unwrap();
    let saves = root.join("saves");

    assert_eq!(install(&t, &saves, "bingo"), Ok(Install::Created));
    let copy = saves.join("bingo/datapacks/bingo");
    assert!(!copy.join("data/bingo/dimension/lobby.json").exists(), "the copy has no lobby dimension");
    assert_eq!(fs::read_to_string(copy.join("data/bingo/dimension_type/lobby.json")).unwrap(), "content of data/bingo/dimension_type/lobby.json");
    assert_eq!(fs::read_to_string(copy.join("data/bingo/worldgen/biome/lobby.json")).unwrap(), "content of data/bingo/worldgen/biome/lobby.json");
    assert_eq!(fs::read_to_string(copy.join("pack.mcmeta")).unwrap(), "content of pack.mcmeta");
    assert!(saves.join("bingo/dimensions/bingo/lobby/region").is_dir());
    // The template, the assets, is never changed.
    assert!(datapack.join("data/bingo/dimension/lobby.json").is_file());

    // Installing again leaves the save as it is.
    assert_eq!(install(&t, &saves, "bingo"), Ok(Install::Exists));
    assert!(!copy.join("data/bingo/dimension/lobby.json").exists());
    assert_eq!(fs::read_to_string(copy.join("pack.mcmeta")).unwrap(), "content of pack.mcmeta");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn nothing_is_made_without_a_template_or_with_a_bad_name_or_a_link() {
    let root = scratch("refuse");
    let saves = root.join("saves");
    assert_eq!(install(&root.join("none"), &saves, "bingo"), Ok(Install::NoTemplate));
    assert!(!saves.join("bingo").exists());
    let t = template(&root);
    assert!(install(&t, &saves, "../escape").is_err());
    assert!(install(&t, &saves, "").is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/", t.join("out")).unwrap();
        assert!(install(&t, &saves, "linked").is_err());
        assert!(!saves.join("linked").exists(), "a refused copy leaves no save");
        assert!(fs::read_dir(&saves).map(|d| d.count()).unwrap_or(0) == 0, "and no temporary folder");
    }
    fs::remove_dir_all(&root).unwrap();
}
