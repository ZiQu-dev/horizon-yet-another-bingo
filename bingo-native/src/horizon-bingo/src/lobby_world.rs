//! The lobby world: the original's saved world "Bingo Lobby", unpacked by `tools/prepare_assets.py` into
//! `<assets>/lobby_world/` (its level.dat, the `bingo` datapack and the lobby dimension's regions). The BINGO! button
//! makes `saves/<folder>` from it when there is no such save, so Horizon can then be asked to open it.
use crate::title_button::lobby_folder_ok;
use std::fs;
use std::path::Path;

/// The lobby's dimension file in the copy's datapack, relative to the save. The engine opens the lobby as a runtime level
/// named `bingo:lobby`, and a datapack dimension of that id would refuse it (`ReservedId`). The dimension type and the
/// biome stay: the saved chunks use them.
const LOBBY_DIMENSION: &str = "datapacks/bingo/data/bingo/dimension/lobby.json";

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
    let temp = saves.join(format!(".{folder}.bingo-new"));
    let _ = fs::remove_dir_all(&temp);
    if let Err(e) = copy_tree(template, &temp).and_then(|()| drop_lobby_dimension(&temp)) {
        let _ = fs::remove_dir_all(&temp);
        return Err(e);
    }
    fs::rename(&temp, &target).map_err(|e| {
        let _ = fs::remove_dir_all(&temp);
        format!("{}: {e}", target.display())
    })?;
    Ok(Install::Created)
}

/// Removes the lobby's datapack dimension from the copy `save` (never from the template). A copy that has none is fine.
fn drop_lobby_dimension(save: &Path) -> Result<(), String> {
    let file = save.join(LOBBY_DIMENSION);
    match fs::remove_file(&file) {
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
