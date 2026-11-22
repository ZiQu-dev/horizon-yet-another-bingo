//! The lobby world's install: `saves/<folder>` is made from the template once, whole, and never over a save. A dedicated
//! server's world gets the lobby's dimension and datapack it lacks (`install_into_world`), and never a file it has.
use horizon_bingo::lobby_world::{install, install_into_world, Install, Part, WorldParts};
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

/// A template with the lobby's dimension and the `bingo` datapack (with its lobby dimension file).
fn lobby_template(root: &PathBuf) -> PathBuf {
    let t = root.join("lobby_world");
    fs::create_dir_all(t.join("dimensions/bingo/lobby/region")).unwrap();
    fs::write(t.join("dimensions/bingo/lobby/region/r.0.0.mca"), b"region").unwrap();
    fs::write(t.join("level.dat"), b"dat").unwrap();
    let datapack = t.join("datapacks/bingo");
    for file in ["data/bingo/dimension/lobby.json", "data/bingo/dimension_type/lobby.json", "pack.mcmeta"] {
        fs::create_dir_all(datapack.join(file).parent().unwrap()).unwrap();
        fs::write(datapack.join(file), format!("content of {file}")).unwrap();
    }
    t
}

#[test]
fn a_world_without_the_lobby_gets_its_dimension_and_datapack_and_the_template_is_kept() {
    let root = scratch("world-fresh");
    let t = lobby_template(&root);
    let world = root.join("world");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"world").unwrap();

    assert_eq!(install_into_world(&t, &world), Ok(WorldParts { dimension: Part::Copied, datapack: Part::Copied }));
    assert_eq!(fs::read(world.join("dimensions/bingo/lobby/region/r.0.0.mca")).unwrap(), b"region");
    let copy = world.join("datapacks/bingo");
    assert!(!copy.join("data/bingo/dimension/lobby.json").exists(), "the copy has no lobby dimension");
    assert_eq!(fs::read_to_string(copy.join("pack.mcmeta")).unwrap(), "content of pack.mcmeta");
    assert_eq!(fs::read_to_string(copy.join("data/bingo/dimension_type/lobby.json")).unwrap(), "content of data/bingo/dimension_type/lobby.json");
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"world", "the world's own files stay");
    // The template is never changed.
    assert!(t.join("datapacks/bingo/data/bingo/dimension/lobby.json").is_file());
    assert!(fs::read_dir(&world).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')), "no temporary folder");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_world_keeps_the_lobby_parts_it_has_and_a_second_install_changes_nothing() {
    let root = scratch("world-kept");
    let t = lobby_template(&root);
    let world = root.join("world");
    // The world has its own dimension (with a played-in region) and its own datapack.
    fs::create_dir_all(world.join("dimensions/bingo/lobby/region")).unwrap();
    fs::write(world.join("dimensions/bingo/lobby/region/r.0.0.mca"), b"played").unwrap();
    fs::create_dir_all(world.join("datapacks/bingo")).unwrap();
    fs::write(world.join("datapacks/bingo/pack.mcmeta"), b"mine").unwrap();

    assert_eq!(install_into_world(&t, &world), Ok(WorldParts { dimension: Part::Kept, datapack: Part::Kept }));
    assert_eq!(fs::read(world.join("dimensions/bingo/lobby/region/r.0.0.mca")).unwrap(), b"played");
    assert_eq!(fs::read(world.join("datapacks/bingo/pack.mcmeta")).unwrap(), b"mine");
    assert!(!world.join("datapacks/bingo/data/bingo/dimension_type/lobby.json").exists(), "nothing of the template is added to a kept datapack");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_missing_part_of_the_template_is_reported_and_not_made() {
    let root = scratch("world-missing");
    let t = root.join("lobby_world");
    fs::create_dir_all(&t).unwrap();
    fs::write(t.join("level.dat"), b"dat").unwrap();
    let world = root.join("world");
    fs::create_dir_all(&world).unwrap();

    assert_eq!(install_into_world(&t, &world), Ok(WorldParts { dimension: Part::Missing, datapack: Part::Missing }));
    assert!(!world.join("dimensions").exists());
    assert!(!world.join("datapacks").exists());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_linked_template_part_is_refused_and_leaves_no_copy() {
    let root = scratch("world-link");
    let t = lobby_template(&root);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/", t.join("dimensions/bingo/lobby/out")).unwrap();
        let world = root.join("world");
        fs::create_dir_all(&world).unwrap();
        assert!(install_into_world(&t, &world).is_err());
        assert!(!world.join("dimensions/bingo/lobby").exists(), "a refused copy leaves no dimension");
        assert!(!world.join("dimensions/bingo/.lobby.bingo-new").exists(), "and no temporary folder");
    }
    fs::remove_dir_all(&root).unwrap();
}
