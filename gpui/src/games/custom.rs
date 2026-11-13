//! Personal executable entries are stored separately from the bundled catalog.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomGame {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub auto_discovered: bool,
    #[serde(default)]
    pub steam_app_id: Option<u32>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub catalog_id: Option<String>,
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
                || e.aliases.len() > 32
                || e.aliases
                    .iter()
                    .any(|p| !p.to_ascii_lowercase().ends_with(".exe"))
        })
    {
        return Err("The custom games file contains invalid entries.".into());
    }
    Ok(entries)
}
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn add(path: &Path) -> Result<CustomGame, String> {
    let _writer = WRITER.lock().map_err(|_| "Game storage is unavailable.")?;
    add_to(&config_path()?, path)
}
pub fn add_discovered(
    path: &Path,
    name: &str,
    steam_app_id: Option<u32>,
) -> Result<CustomGame, String> {
    let _writer = WRITER.lock().map_err(|_| "Game storage is unavailable.")?;
    add_to_named(&config_path()?, path, Some((name, steam_app_id)))
}
fn add_to(config: &Path, selected: &Path) -> Result<CustomGame, String> {
    add_to_named(config, selected, None)
}
fn add_to_named(
    config: &Path,
    selected: &Path,
    discovered: Option<(&str, Option<u32>)>,
) -> Result<CustomGame, String> {
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
    if let Some(existing) = entries.iter().find(|e| {
        crate::macros::paths_match(&e.path, &path)
            || e.aliases
                .iter()
                .any(|p| crate::macros::paths_match(p, &path))
    }) {
        return Ok(existing.clone());
    }
    if let Some((_, Some(app))) = discovered {
        if let Some(index) = entries.iter().position(|e| {
            e.steam_app_id
                .or_else(|| super::install_scan::steam_app_for_executable(&e.path))
                == Some(app)
        }) {
            if !entries[index]
                .aliases
                .iter()
                .any(|p| crate::macros::paths_match(p, &path))
            {
                if entries[index].aliases.len() >= 32 {
                    return Err("Too many executables for this game.".into());
                }
                entries[index].aliases.push(path);
                save(config, &entries)?;
            }
            return Ok(entries[index].clone());
        }
    }
    if entries.len() >= 256 {
        return Err("At most 256 custom games are supported.".into());
    }
    let stem = selected
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("The executable name is not readable.")?;
    let mut name: String = stem.replace(['_', '-'], " ").chars().take(100).collect();
    if let Some((title, _)) = discovered {
        if !title.trim().is_empty() {
            name = title.trim().chars().take(100).collect();
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let entry = CustomGame {
        id: format!("custom:{stamp:x}-{:x}", std::process::id()),
        name,
        path,
        auto_discovered: discovered.is_some(),
        steam_app_id: discovered.and_then(|(_, id)| id),
        aliases: Vec::new(),
        catalog_id: discovered
            .and_then(|(_, id)| id)
            .and_then(|id| super::bundled_catalog_id(id)),
    };
    entries.push(entry.clone());
    save(config, &entries)?;
    Ok(entry)
}
fn save(path: &Path, entries: &[CustomGame]) -> Result<(), String> {
    save_value(path, entries)
}
pub(super) fn save_value(path: &Path, entries: &(impl Serialize + ?Sized)) -> Result<(), String> {
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

#[cfg(test)]
mod discovery_tests {
    use super::*;
    #[test]
    fn discovery_reuses_persisted_identity_and_steam_executable_variants() {
        let root = std::env::temp_dir().join(format!(
            "openmouse-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("Game.exe");
        let second = root.join("GameDX12.exe");
        std::fs::write(&first, b"fixture").unwrap();
        std::fs::write(&second, b"fixture").unwrap();
        let config = root.join("games.json");
        let original =
            add_to_named(&config, &first, Some(("Test title", Some(4294967000)))).unwrap();
        let again =
            add_to_named(&config, &first, Some(("Different title", Some(4294967000)))).unwrap();
        let variant =
            add_to_named(&config, &second, Some(("Test title", Some(4294967000)))).unwrap();
        assert_eq!(original.id, again.id);
        assert_eq!(original.id, variant.id);
        assert_eq!(load_from(&config).unwrap().len(), 1);
        assert_eq!(variant.aliases.len(), 1);
        assert_eq!(add_to(&config, &second).unwrap().id, original.id);
        assert_eq!(variant.name, "Test title");
        assert!(variant.auto_discovered);
        assert_eq!(
            add_to_named(&config, &second, Some(("Test title", Some(4294967000))))
                .unwrap()
                .aliases
                .len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn old_records_are_manual_and_discovery_does_not_overwrite_them() {
        let old: CustomGame = serde_json::from_str(
            r#"{"id":"custom:old","name":"My title","path":"C:\\Games\\Game.exe"}"#,
        )
        .unwrap();
        assert!(!old.auto_discovered);
        assert!(old.aliases.is_empty());
        assert!(old.catalog_id.is_none());
    }
}

/// Keep profiles with macros; consolidate only empty duplicate records.
pub fn cleanup_empty_duplicates(
    with_macros: &std::collections::HashSet<String>,
) -> Result<Vec<(String, String)>, String> {
    let _writer = WRITER.lock().map_err(|_| "Game storage is unavailable.")?;
    let config = config_path()?;
    let mut entries = load_from(&config)?;
    let artwork = super::profile_art::load()?;
    let hidden = super::deleted::load()?;
    let identities: Vec<_> = entries
        .iter()
        .map(|e| {
            e.steam_app_id
                .or_else(|| artwork.get(&e.id).and_then(|a| a.steam_app_id))
                .or_else(|| super::install_scan::steam_app_for_executable(&e.path))
        })
        .collect();
    let hidden_ids = hidden.iter().map(|d| d.id.clone()).collect();
    let backup = entries.clone();
    let removed = consolidate_duplicates(&mut entries, &identities, with_macros, &hidden_ids);
    if !removed.is_empty() {
        // Keep pre-cleanup metadata for recovery; macro contents are never edited.
        save(
            &config.with_file_name("duplicate-profiles-backup.json"),
            &backup,
        )?;
        save(&config, &entries)?;
    }
    Ok(removed)
}
fn consolidate_duplicates(
    entries: &mut Vec<CustomGame>,
    steam: &[Option<u32>],
    with_macros: &std::collections::HashSet<String>,
    hidden: &std::collections::HashSet<String>,
) -> Vec<(String, String)> {
    let mut gone = std::collections::HashSet::new();
    let mut removed = Vec::new();
    for i in 0..entries.len() {
        if gone.contains(&i) || hidden.contains(&entries[i].id) {
            continue;
        }
        for j in i + 1..entries.len() {
            if gone.contains(&i) {
                break;
            }
            if gone.contains(&j) || hidden.contains(&entries[j].id) {
                continue;
            }
            let same = (steam[i].is_some() && steam[i] == steam[j])
                || crate::macros::paths_match(&entries[i].path, &entries[j].path);
            if !same {
                continue;
            }
            let a =
                with_macros.contains(entries[i].catalog_id.as_deref().unwrap_or(&entries[i].id));
            let b =
                with_macros.contains(entries[j].catalog_id.as_deref().unwrap_or(&entries[j].id));
            if a && b {
                continue;
            }
            let (keep, drop) =
                if b || (!a && entries[i].auto_discovered && !entries[j].auto_discovered) {
                    (j, i)
                } else {
                    (i, j)
                };
            let paths = std::iter::once(entries[drop].path.clone())
                .chain(entries[drop].aliases.clone())
                .collect::<Vec<_>>();
            for path in paths {
                if !crate::macros::paths_match(&entries[keep].path, &path)
                    && !entries[keep]
                        .aliases
                        .iter()
                        .any(|p| crate::macros::paths_match(p, &path))
                {
                    if entries[keep].aliases.len() < 32 {
                        entries[keep].aliases.push(path);
                    }
                }
            }
            entries[keep].steam_app_id = entries[keep].steam_app_id.or(steam[keep]);
            removed.push((entries[drop].id.clone(), entries[keep].id.clone()));
            gone.insert(drop);
        }
    }
    let mut index = 0;
    entries.retain(|_| {
        let keep = !gone.contains(&index);
        index += 1;
        keep
    });
    // Resolve keeper chains when several empty records were consolidated.
    for i in 0..removed.len() {
        for _ in 0..removed.len() {
            let next = removed
                .iter()
                .find(|(old, _)| old == &removed[i].1)
                .map(|(_, new)| new.clone());
            if let Some(next) = next {
                removed[i].1 = next;
            } else {
                break;
            }
        }
    }
    removed
}
#[cfg(test)]
mod duplicate_cleanup_tests {
    use super::*;
    fn entry(id: &str) -> CustomGame {
        CustomGame {
            id: id.into(),
            name: id.into(),
            path: format!("D:\\Games\\{id}.exe"),
            auto_discovered: true,
            steam_app_id: Some(553850),
            aliases: vec![],
            catalog_id: None,
        }
    }
    #[test]
    fn keeps_macros_and_both_nonempty_profiles_are_preserved() {
        let mut entries = vec![entry("original"), entry("duplicate")];
        let with_macros = std::collections::HashSet::from(["original".into()]);
        let removed = consolidate_duplicates(
            &mut entries,
            &[Some(553850), Some(553850)],
            &with_macros,
            &Default::default(),
        );
        assert_eq!(removed, vec![("duplicate".into(), "original".into())]);
        assert_eq!(entries[0].id, "original");
        assert_eq!(entries[0].aliases.len(), 1);
        let mut entries = vec![entry("original"), entry("duplicate")];
        let both = std::collections::HashSet::from(["original".into(), "duplicate".into()]);
        assert!(consolidate_duplicates(
            &mut entries,
            &[Some(553850), Some(553850)],
            &both,
            &Default::default()
        )
        .is_empty());
        assert_eq!(entries.len(), 2);
    }
}

#[cfg(test)]
mod existing_manual_regression {
    use super::*;
    #[test]
    fn helldivers_manual_profile_reuses_steam_identity_and_keeps_its_id() {
        let root = std::env::temp_dir().join(format!(
            "openmouse-hd2-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let folder = root.join("steamapps/common/Helldivers 2/bin");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(root.join("steamapps/appmanifest_553850.acf"),r#""AppState" { "appid" "553850" "installdir" "Helldivers 2" "name" "HELLDIVERS 2" "StateFlags" "4" }"#).unwrap();
        let original = folder.join("helldivers2.exe");
        let launched = folder.join("helldivers2_alt.exe");
        std::fs::write(&original, b"fixture").unwrap();
        std::fs::write(&launched, b"fixture").unwrap();
        let config = root.join("games.json");
        let manual = add_to(&config, &original).unwrap();
        let matched =
            add_to_named(&config, &launched, Some(("HELLDIVERS 2", Some(553850)))).unwrap();
        assert_eq!(manual.id, matched.id);
        assert!(!matched.auto_discovered);
        assert_eq!(load_from(&config).unwrap().len(), 1);
        assert_eq!(matched.aliases.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
