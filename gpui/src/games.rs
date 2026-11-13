//! Game catalog: static list + install detection.
//!
//! Mirrors `use-game-watcher.ts`'s `Game` shape and `public/games.json`.
//! The catalog ships embedded (no network at startup); artwork URLs resolve
//! lazily per card via `img()`. Native install detection reads launcher
//! manifests on macOS and Windows without touching artwork or running state.

use serde::{Deserialize, Serialize};

#[path = "games/custom.rs"]
pub mod custom;
#[path = "games/discovery.rs"]
pub mod discovery;
#[path = "games/deleted.rs"]
pub mod deleted;
#[path = "games/executable_icon.rs"]
pub mod executable_icon;
#[path = "games/profile_art.rs"]
pub mod profile_art;
#[path = "games/install_scan.rs"]
mod install_scan;

/// One catalog entry, mirroring the TS `Game` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artwork: Option<String>,
    #[serde(default, rename = "steamAppId", deserialize_with = "de_steam_id")]
    pub steam_app_id: Option<u32>,
    #[serde(default, rename = "epicId")]
    pub epic_id: Option<String>,
    #[serde(default, rename = "riotId")]
    pub riot_id: Option<String>,
    #[serde(default)]
    pub executables: Vec<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)] pub profile_image: Option<String>,
    #[serde(skip)]
    pub installed: bool,
    #[serde(skip)]
    pub running: bool,
}

impl Game {
    /// Card art URL: explicit artwork, else Steam CDN cover. Mirrors the
    /// `GameCard` src chain (`artwork ?? steam library_600x900_2x.jpg`).
    pub fn art_url(&self) -> Option<String> {
        if let Some(url) = self.artwork.as_ref() {
            return Some(url.clone());
        }
        self.steam_app_id.map(|id| {
            format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900_2x.jpg")
        })
    }

    /// Fallback letter when no art resolves (mirrors `.game-card-mark`).
    pub fn mark(&self) -> char {
        self.name.chars().next().unwrap_or('?')
    }
}

/// `steamAppId` is inconsistently typed in the catalog (number for some
/// titles, string for others) — accept both.
fn de_steam_id<'de, D>(d: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flexible {
        Number(u32),
        Text(String),
        Empty,
    }
    match Flexible::deserialize(d).unwrap_or(Flexible::Empty) {
        Flexible::Number(id) => Ok(Some(id)),
        Flexible::Text(text) => Ok(text.trim().parse::<u32>().ok()),
        Flexible::Empty => Ok(None),
    }
}

/// On-disk art cache: `<cache_dir>/openmouse/game-art/<id>.jpg`.
/// Prefetched at startup on a background thread (mirrors the web app's lazy
/// `<img loading="lazy">`, but ahead-of-time: one small batch instead of
/// per-card fetches during scroll). Cards read `cached_art_path` and fall
/// back to the letter tile until the file lands.
pub fn art_cache_dir() -> Option<std::path::PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("openmouse").join("game-art"))
}

// Include an explicit source in cache identity so catalog artwork changes
// cannot keep displaying the previously downloaded image.
fn art_cache_path(dir: &std::path::Path, game: &Game) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    if let Some(executable) = &game.executable_path {
        if let Some(id) = game.steam_app_id { return dir.join(format!("steam-cover-{id}.png")); }
        return executable_icon::cache_path(dir, executable);
    }
    match game.artwork.as_deref() {
        Some(url) => {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            url.hash(&mut hash);
            dir.join(format!("{}-{:016x}.jpg", game.id, hash.finish()))
        }
        None => dir.join(format!("{}.jpg", game.id)),
    }
}

/// Local art file for a game, if already prefetched.
pub fn cached_art_path(game: &Game) -> Option<std::path::PathBuf> {
    if let Some(file) = &game.profile_image { let path = std::path::PathBuf::from(file); if path.is_file() { return Some(path); } }
    let dir = art_cache_dir()?;
    let path = art_cache_path(&dir, game);
    if path.is_file() { return Some(path); }
    game.executable_path.as_ref().map(|exe| executable_icon::cache_path(&dir, exe)).filter(|p| p.is_file())
}

pub fn cache_executable_icon(executable: &str) {
    if let Some(dir) = art_cache_dir() {
        let target = executable_icon::cache_path(&dir, executable);
        executable_icon::extract(executable, &target);
    }
}

pub fn auto_cover(id: &str, force: bool) -> Result<(), String> {
    let preferences = profile_art::load()?;
    let choice = preferences.get(id).cloned().unwrap_or_default();
    if !force && choice.image_file.is_some() { return Ok(()); }
    if !force && choice.checked && choice.steam_app_id.is_none() { return Ok(()); }
    let game = load_catalog().into_iter().find(|g| g.id == id).ok_or("Game no longer exists.")?;
    let app_id = choice.steam_app_id.or(game.steam_app_id).or_else(|| game.executable_path.as_deref().and_then(install_scan::steam_app_for_executable));
    let Some(app_id) = app_id else { profile_art::record_lookup(id, None, false)?; return Err("No matching Steam installation was found. Use Choose image.".into()); };
    profile_art::record_lookup(id, Some(app_id), false)?;
    profile_art::download_cover(app_id, &art_cache_dir().ok_or("No artwork folder is available.")?)?;
    profile_art::record_lookup(id, Some(app_id), force)
}
pub fn uses_executable_icon(game: &Game) -> bool {
    let Some(path) = cached_art_path(game) else { return false; };
    let Some(exe) = &game.executable_path else { return false; };
    art_cache_dir().is_some_and(|dir| path == executable_icon::cache_path(&dir, exe))
}

/// Download missing art for all games with a resolvable URL. Blocking;
/// callers must run this off the UI thread. Uses the system `curl`
/// (dependency-free) with a per-file timeout; failures are skipped, leaving
/// the letter tile in place.
pub fn prefetch_art(games: &[Game]) {
    let Some(dir) = art_cache_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    // Personal executable icons are local and should not wait on cover downloads.
    for game in games {
        if let Some(executable) = &game.executable_path {
            cache_executable_icon(executable);
        }
    }
    for game in games {
        if game.profile_image.is_some() { continue; }
        if game.executable_path.is_some() {
            if let Some(id) = game.steam_app_id { let _ = profile_art::download_cover(id, &dir); }
            continue;
        }
        let Some(url) = game.art_url() else { continue };
        let target = art_cache_path(&dir, game);
        if target.is_file() {
            continue;
        }
        let tmp = dir.join(format!("{}.part", game.id));
        let status = crate::proc::quiet(
            std::process::Command::new("curl")
                .args(["-sSfL", "-m", "20", "-o"])
                .arg(&tmp)
                .arg(&url)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()),
        )
        .status();
        if status.is_ok_and(|s| s.success()) && tmp.is_file() {
            let _ = std::fs::rename(&tmp, &target);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[derive(Deserialize)]
struct GamesFile {
    games: Vec<Game>,
}

/// Embedded catalog (`public/games.json` at build time).
const GAMES_JSON: &str = include_str!("../../public/games.json");

pub(super) fn bundled_catalog_id(steam_app_id:u32) -> Option<String> {
    serde_json::from_str::<GamesFile>(GAMES_JSON).ok()?.games.into_iter().find(|g|g.steam_app_id==Some(steam_app_id)).map(|g|g.id)
}
/// Load the catalog; `installed` starts false until a scan completes.
pub fn load_catalog() -> Vec<Game> {
    let mut games = match serde_json::from_str::<GamesFile>(GAMES_JSON) {
        Ok(file) => file.games,
        Err(e) => {
            eprintln!("games catalog failed to parse: {e}");
            Vec::new()
        }
    };
    match custom::load() {
        Ok(entries) => {
            for e in entries {
                if let Some(id)=&e.catalog_id {
                    if let Some(game)=games.iter_mut().find(|g|&g.id==id) {
                        game.executables.push(e.path.clone()); game.executables.extend(e.aliases);
                        continue;
                    }
                }
                let executable = e
                    .path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(&e.path)
                    .to_string();
                let installed = std::path::Path::new(&e.path).is_file();
                games.push(Game {
                    id: e.id,
                    name: e.name,
                    artwork: None,
                    profile_image: None,
                    steam_app_id: e.steam_app_id,
                    epic_id: None,
                    riot_id: None,
                    executables: std::iter::once(executable).chain(e.aliases).collect(),
                    executable_path: Some(e.path),
                    installed,
                    running: false,
                });
            }
        }
        Err(e) => eprintln!("{e}"),
    }
    if let Ok(preferences) = profile_art::load() {
        for game in &mut games { if let Some(choice) = preferences.get(&game.id) {
            game.profile_image = profile_art::chosen_image(choice).map(|p| p.to_string_lossy().to_string());
            if let Some(id) = choice.steam_app_id { game.steam_app_id = Some(id);
                if game.profile_image.is_none() { game.profile_image = art_cache_dir().map(|dir| dir.join(format!("steam-cover-{id}.png"))).filter(|p| p.is_file()).map(|p| p.to_string_lossy().to_string()); }
            }
        }}
    }
    if let Ok(deleted)=deleted::load() {games.retain(|g|!deleted.iter().any(|d|d.id==g.id));}
    games
}
pub fn macro_targets(games: &[Game]) -> Vec<crate::macros::GameTarget> {
    games
        .iter()
        .flat_map(|g| {
            let mut targets=vec![crate::macros::GameTarget {id:g.id.clone(),executables:g.executables.clone(),path:g.executable_path.clone()}];
            for path in &g.executables {
                if path.contains(['\\','/']) {targets.push(crate::macros::GameTarget {id:g.id.clone(),executables:Vec::new(),path:Some(path.clone())});}
            }
            targets
        })
        .collect()
}

/// Refresh installed state from Steam, Epic, and Riot manifests. Blocking;
/// callers must run this off the UI thread. Missing launchers clear old state.
pub fn mark_installed(games: &mut [Game]) {
    install_scan::mark_installed(games);
    for game in games {
        if let Some(path) = &game.executable_path {
            game.installed = std::path::Path::new(path).is_file();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_with_art_chain() {
        let games = load_catalog();
        assert!(!games.is_empty());
        let cs2 = games.iter().find(|g| g.id == "counter-strike-2").unwrap();
        assert_eq!(
            cs2.art_url().as_deref(),
            Some("https://cdn.cloudflare.steamstatic.com/steam/apps/730/library_600x900_2x.jpg")
        );
        assert_eq!(cs2.mark(), 'C');
    }

    #[test]
    fn catalog_retains_launcher_ids_and_skips_runtime_state() {
        let games = load_catalog();
        assert_eq!(
            games
                .iter()
                .find(|game| game.id == "fortnite")
                .unwrap()
                .epic_id
                .as_deref(),
            Some("Fortnite")
        );
        assert_eq!(
            games
                .iter()
                .find(|game| game.id == "league-of-legends")
                .unwrap()
                .riot_id
                .as_deref(),
            Some("league_of_legends")
        );
        assert!(games.iter().all(|game| !game.installed && !game.running));
        let mut game = games[0].clone();
        game.installed = true;
        game.running = true;
        let json = serde_json::to_value(game).unwrap();
        assert!(json.get("installed").is_none());
        assert!(json.get("running").is_none());
        assert!(json.get("epicId").is_some());
        assert!(json.get("riotId").is_some());
    }

    #[test]
    fn explicit_artwork_wins_over_steam() {
        let game = Game {
            executable_path: None,
            profile_image: None,
            id: "x".into(),
            name: "X".into(),
            artwork: Some("https://example.com/a.jpg".into()),
            steam_app_id: Some(1),
            epic_id: None,
            riot_id: None,
            executables: vec![],
            installed: false,
            running: false,
        };
        assert_eq!(game.art_url().as_deref(), Some("https://example.com/a.jpg"));
    }
}

/// Human-readable notification title; never changes a saved profile or its identity.
pub fn notification_title(name: &str) -> String {
    let name = name.trim();
    let stem = if name.to_ascii_lowercase().ends_with(".exe") {
        &name[..name.len() - 4]
    } else { name };
    let identity: String = stem.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    if identity == "helldivers2" { return "HellDivers 2".into(); }
    if stem.contains(' ') && !stem.contains(['_', '-']) && stem.chars().any(char::is_uppercase) {
        return stem.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let mut spaced = String::new();
    let mut previous = None;
    for c in stem.chars() {
        if c == '_' || c == '-' || c.is_whitespace() {
            if !spaced.is_empty() && !spaced.ends_with(' ') { spaced.push(' '); }
            previous = None;
            continue;
        }
        if previous.is_some_and(|p: char| p.is_alphabetic() && c.is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(c);
        previous = Some(c);
    }
    spaced.trim().split_whitespace().map(|word| {
        // Preserve existing capitalization, including acronyms and branded names.
        if word.chars().any(char::is_uppercase) { return word.to_string(); }
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod notification_title_tests {
    use super::notification_title;
    #[test]
    fn readable_titles_preserve_existing_branding() {
        for (raw, expected) in [
            ("helldivers2", "HellDivers 2"),
            ("HELLDIVERS 2", "HellDivers 2"),
            ("helldivers2.exe", "HellDivers 2"),
            ("counter_strike-2", "Counter Strike 2"),
            ("portal2", "Portal 2"),
            ("GTA V", "GTA V"),
            ("Cyberpunk 2077", "Cyberpunk 2077"),
            ("  Sea of Thieves  ", "Sea of Thieves"),
        ] { assert_eq!(notification_title(raw), expected); }
    }
}
