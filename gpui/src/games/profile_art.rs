//! Persistent profile artwork; manually chosen images always take priority.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub steam_app_id: Option<u32>,
    pub image_file: Option<String>,
    pub checked: bool,
}
pub fn directory() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("openmouse").join("profile-images"))
        .ok_or_else(|| "No configuration folder is available.".into())
}
pub fn load() -> Result<BTreeMap<String, Choice>, String> {
    load_from(&directory()?.join("profiles.json"))
}
fn load_from(path: &Path) -> Result<BTreeMap<String, Choice>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Could not read profile artwork: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn chosen_image(choice: &Choice) -> Option<PathBuf> {
    let file = choice.image_file.as_ref()?;
    if Path::new(file).file_name()?.to_str()? != file || file.contains(['/', '\\']) {
        return None;
    }
    let path = directory().ok()?.join(file);
    path.is_file().then_some(path)
}
fn save(entries: &BTreeMap<String, Choice>) -> Result<(), String> {
    let dir = directory()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("profiles.json");
    let temp = dir.join("profiles.json.tmp");
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
    std::fs::rename(temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn record_lookup(
    id: &str,
    steam_app_id: Option<u32>,
    replace_manual: bool,
) -> Result<(), String> {
    let _writer = WRITER
        .lock()
        .map_err(|_| "Artwork storage is unavailable.")?;
    let mut entries = load()?;
    let choice = entries.entry(id.into()).or_default();
    choice.steam_app_id = steam_app_id.or(choice.steam_app_id);
    choice.checked = true;
    if replace_manual {
        choice.image_file = None;
    }
    save(&entries)
}
fn encode_image(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Choose an image smaller than 20 MB.".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader
        .decode()
        .map_err(|_| "Choose a valid PNG, JPEG or BMP image.".to_string())?
        .thumbnail(600, 900);
    let mut output = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}
pub fn choose_image(id: &str, source: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(source)
        .map_err(|e| e.to_string())?
        .take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let png = encode_image(bytes)?;
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    png.hash(&mut hash);
    let file = format!("chosen-{:016x}.png", hash.finish());
    let dir = directory()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _writer = WRITER
        .lock()
        .map_err(|_| "Artwork storage is unavailable.")?;
    let mut entries = load()?;
    std::fs::write(dir.join(&file), png).map_err(|e| e.to_string())?;
    entries.entry(id.into()).or_default().image_file = Some(file);
    save(&entries)
}
pub fn download_cover(app_id: u32, directory: &Path) -> Result<(), String> {
    let target = directory.join(format!("steam-cover-{app_id}.png"));
    if target.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = directory.join(format!("steam-cover-{app_id}.download"));
    for suffix in [
        "library_600x900_2x.jpg",
        "library_600x900.jpg",
        "header.jpg",
    ] {
        let url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/{suffix}");
        let mut command = crate::proc::curl();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let status = command
            .args([
                "-sSfL",
                "--proto",
                "=https",
                "--max-redirs",
                "2",
                "--max-filesize",
                "8388608",
                "-m",
                "8",
                "-o",
            ])
            .arg(&temporary)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.is_ok_and(|s| s.success()) {
            if let Ok(bytes) = std::fs::read(&temporary) {
                if let Ok(png) = encode_image(bytes) {
                    std::fs::write(&target, png).map_err(|e| e.to_string())?;
                    let _ = std::fs::remove_file(&temporary);
                    return Ok(());
                }
            }
        }
        let _ = std::fs::remove_file(&temporary);
    }
    Err("Steam artwork could not be downloaded. Choose an image or try again later.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_resizes_images_instead_of_copying_arbitrary_files() {
        assert!(encode_image(b"not an image".to_vec()).is_err());
        let image = image::DynamicImage::new_rgba8(1200, 1600);
        let mut input = std::io::Cursor::new(Vec::new());
        image.write_to(&mut input, image::ImageFormat::Png).unwrap();
        let png = encode_image(input.into_inner()).unwrap();
        let resized = image::load_from_memory(&png).unwrap();
        assert!(resized.width() <= 600);
        assert!(resized.height() <= 900);
    }
    #[test]
    fn old_artwork_preferences_can_be_read_without_manual_images() {
        let c: Choice = serde_json::from_str(r#"{"steam_app_id":553850}"#).unwrap();
        assert_eq!(c.steam_app_id, Some(553850));
        assert!(c.image_file.is_none());
    }
}
