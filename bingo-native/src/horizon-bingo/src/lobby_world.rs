//! The lobby world: the original's saved world "Bingo Lobby", unpacked by `tools/prepare_assets.py` into
//! `<assets>/lobby_world/` (its level.dat, the `bingo` datapack and the lobby dimension's regions). The BINGO! button
//! makes `saves/<folder>` from it when there is no such save, so Horizon can then be asked to open it. A dedicated
//! server's world gets the lobby's parts it does not have yet (`install_into_world`).
use crate::title_button::lobby_folder_ok;
use std::fs;
use std::path::Path;

/// The lobby's dimension folder in a save, and in the template (the saved chunks of the lobby).
const DIMENSION_DIR: &str = "dimensions/bingo/lobby";
/// The `bingo` datapack's folder in a save, and in the template.
const DATAPACK_DIR: &str = "datapacks/bingo";
/// The lobby's dimension file in the datapack, relative to the datapack folder. The engine opens the lobby as a runtime
/// level named `bingo:lobby`, and a datapack dimension of that id would refuse it (`ReservedId`). The dimension type and
/// the biome stay: the saved chunks use them.
const LOBBY_DIMENSION_IN_DATAPACK: &str = "data/bingo/dimension/lobby.json";

/// What `install` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Install {
    /// `saves/<folder>` was made from the template.
    Created,
    /// `saves/<folder>` is there already (a save of any kind, or a file): nothing is touched.
    Exists,
    /// The template is not there (assets prepared without the original's jar's lobby): nothing is made.
    NoTemplate,
}

/// What one part of the lobby did in a world (`install_into_world`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The part was copied from the template.
    Copied,
    /// The world has the part already: nothing is touched.
    Kept,
    /// The template has no such part: nothing is made.
    Missing,
}

/// The lobby's two parts in a world: its dimension (the saved chunks) and the datapack (the dimension type, the biome and
/// `pack.mcmeta`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldParts {
    pub dimension: Part,
    pub datapack: Part,
}

/// Copies the template folder `template` to `saves/<folder>` unless that exists. The copy is made beside it under a
/// temporary name and renamed, so a game never sees half a save. Links in the template are refused, the session lock
/// is not copied, and a bad folder name is an error.
pub fn install(template: &Path, saves: &Path, folder: &str) -> Result<Install, String> {
    if !lobby_folder_ok(folder) {
        return Err(format!("{folder:?} is not a plain folder name"));
    }
    let target = saves.join(folder);
    if fs::symlink_metadata(&target).is_ok() {
        return Ok(Install::Exists);
    }
    if !template.join("level.dat").is_file() {
        return Ok(Install::NoTemplate);
    }
    fs::create_dir_all(saves).map_err(|e| format!("{}: {e}", saves.display()))?;
    copy_part(template, &target, |save| remove_file_if_present(&save.join(LOBBY_DIMENSION_IN_SAVE)))?;
    Ok(Install::Created)
}

/// Gives the save `world` (a server's world folder) the lobby's parts it does not have: `dimensions/bingo/lobby` and
/// `datapacks/bingo` from the template, the datapack without the lobby's dimension file. A part the world has is never
/// overwritten, and a part the template lacks is not made. The template is never changed.
pub fn install_into_world(template: &Path, world: &Path) -> Result<WorldParts, String> {
    let dimension = copy_part(&template.join(DIMENSION_DIR), &world.join(DIMENSION_DIR), |_| Ok(()))?;
    let datapack = copy_part(&template.join(DATAPACK_DIR), &world.join(DATAPACK_DIR), |copy| {
        remove_file_if_present(&copy.join(LOBBY_DIMENSION_IN_DATAPACK))
    })?;
    Ok(WorldParts { dimension, datapack })
}

/// The lobby's dimension file relative to a save (its datapack's file).
const LOBBY_DIMENSION_IN_SAVE: &str = "datapacks/bingo/data/bingo/dimension/lobby.json";

/// Copies the folder `from` to `to` when `to` does not exist. The copy is made beside `to` under a temporary name, `after`
/// may change it (the copy only, never `from`), and then it is renamed to `to`, so nothing sees half a copy. A `from`
/// that is not a folder is `Missing`; a `to` that exists in any form is `Kept`.
fn copy_part(from: &Path, to: &Path, after: impl FnOnce(&Path) -> Result<(), String>) -> Result<Part, String> {
    if fs::symlink_metadata(to).is_ok() {
        return Ok(Part::Kept);
    }
    if !from.is_dir() {
        return Ok(Part::Missing);
    }
    let parent = to.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = parent.join(format!(".{name}.bingo-new"));
    let _ = fs::remove_dir_all(&temp);
    if let Err(e) = copy_tree(from, &temp).and_then(|()| after(&temp)) {
        let _ = fs::remove_dir_all(&temp);
        return Err(e);
    }
    fs::rename(&temp, to).map_err(|e| {
        let _ = fs::remove_dir_all(&temp);
        format!("{}: {e}", to.display())
    })?;
    Ok(Part::Copied)
}

/// Removes the file `file` from a copy. A copy that has none is fine.
fn remove_file_if_present(file: &Path) -> Result<(), String> {
    match fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "session.lock" {
            continue;
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let (src, dst) = (entry.path(), to.join(&name));
        if kind.is_symlink() {
            return Err(format!("{}: a link in the lobby template", src.display()));
        } else if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}
