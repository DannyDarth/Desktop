//! Personal executable entries are stored separately from the bundled catalog.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomGame {
    pub id: String,
    pub name: String,
    pub path: String,
}
fn config_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("openmouse").join("custom-games.json"))
        .ok_or_else(|| "No configuration folder is available.".into())
}
pub fn load() -> Result<Vec<CustomGame>, String> {
    load_from(&config_path()?)
}
fn load_from(path: &Path) -> Result<Vec<CustomGame>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let entries: Vec<CustomGame> =
        serde_json::from_slice(&bytes).map_err(|e| format!("Could not read custom games: {e}"))?;
    if entries.len() > 256
        || entries.iter().any(|e| {
            !e.id.starts_with("custom:")
                || e.name.is_empty()
                || e.name.chars().count() > 100
                || !e.path.to_ascii_lowercase().ends_with(".exe")
        })
    {
        return Err("The custom games file contains invalid entries.".into());
    }
    Ok(entries)
}
pub fn add(path: &Path) -> Result<CustomGame, String> {
    add_to(&config_path()?, path)
}
fn add_to(config: &Path, selected: &Path) -> Result<CustomGame, String> {
    if !selected.is_file() {
        return Err("Choose the game's executable file.".into());
    }
    if !selected
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err("Choose a .exe file, not a shortcut or launcher folder.".into());
    }
    let path = std::fs::canonicalize(selected)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string();
    let mut entries = load_from(config)?;
    if let Some(existing) = entries
        .iter()
        .find(|e| crate::macros::paths_match(&e.path, &path))
    {
        return Ok(existing.clone());
    }
    if entries.len() >= 256 {
        return Err("At most 256 custom games are supported.".into());
    }
    let stem = selected
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("The executable name is not readable.")?;
    let name: String = stem.replace(['_', '-'], " ").chars().take(100).collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let entry = CustomGame {
        id: format!("custom:{stamp:x}-{:x}", std::process::id()),
        name,
        path,
    };
    entries.push(entry.clone());
    save(config, &entries)?;
    Ok(entry)
}
fn save(path: &Path, entries: &[CustomGame]) -> Result<(), String> {
    std::fs::create_dir_all(
        path.parent()
            .ok_or("No configuration folder is available.")?,
    )
    .map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let a: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 9) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn added_game_survives_reload_and_duplicate_selection() {
        let root = std::env::temp_dir().join(format!(
            "openmouse-games-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("helldivers2.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let config = root.join("custom-games.json");
        let first = add_to(&config, &exe).unwrap();
        let second = add_to(&config, &exe).unwrap();
        assert_eq!(first.id, second.id);
        let entries = load_from(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, first.path);
        assert!(add_to(&config, &root).is_err());
        let shortcut = root.join("game.lnk");
        std::fs::write(&shortcut, b"fixture").unwrap();
        assert!(add_to(&config, &shortcut).is_err());
        std::fs::write(&config, b"broken").unwrap();
        assert!(add_to(&config, &exe).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), b"broken");
        std::fs::remove_dir_all(root).unwrap();
    }
}
