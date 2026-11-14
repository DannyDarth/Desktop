//! Persistent opt-out: deleted profiles must not be recreated by discovery.
use super::Game;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeletedProfile {
    pub id: String,
    pub steam_app_id: Option<u32>,
    pub executables: Vec<String>,
}
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn path() -> Result<std::path::PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("openmouse/deleted-games.json"))
        .ok_or("No configuration folder is available.".into())
}
pub fn load() -> Result<Vec<DeletedProfile>, String> {
    match std::fs::read(path()?) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Could not read deleted profiles: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn matches(entry: &DeletedProfile, executable: &str, steam: Option<u32>) -> bool {
    (steam.is_some() && entry.steam_app_id == steam)
        || entry.executables.iter().any(|e| {
            if e.contains(['\\', '/']) {
                crate::macros::paths_match(e, executable)
            } else {
                executable
                    .rsplit(['\\', '/'])
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(e))
            }
        })
}
pub fn blocked(executable: &str, steam: Option<u32>) -> Result<bool, String> {
    Ok(load()?
        .iter()
        .any(|entry| matches(entry, executable, steam)))
}
pub fn delete(game: &Game) -> Result<(), String> {
    let _lock = WRITER
        .lock()
        .map_err(|_| "Profile storage is unavailable.")?;
    let mut entries = load()?;
    if !entries.iter().any(|e| e.id == game.id) {
        let mut executables = game.executables.clone();
        if let Some(path) = &game.executable_path {
            executables.push(path.clone());
        }
        entries.push(DeletedProfile {
            id: game.id.clone(),
            steam_app_id: game.steam_app_id,
            executables,
        });
        super::custom::save_value(&path()?, &entries)?;
    }
    Ok(())
}
pub fn restore(id: &str) -> Result<(), String> {
    let _lock = WRITER
        .lock()
        .map_err(|_| "Profile storage is unavailable.")?;
    let mut entries = load()?;
    entries.retain(|e| e.id != id);
    super::custom::save_value(&path()?, &entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleted_game_is_blocked_by_identity_without_hiding_unrelated_paths() {
        let entry = DeletedProfile {
            id: "custom:game".into(),
            steam_app_id: Some(553850),
            executables: vec![r"D:\Games\Helldivers\game.exe".into()],
        };
        assert!(matches(&entry, r"d:/games/helldivers/game.exe", None));
        assert!(matches(&entry, r"E:\New location\game.exe", Some(553850)));
        assert!(!matches(&entry, r"D:\Other\game.exe", None));
    }
}
