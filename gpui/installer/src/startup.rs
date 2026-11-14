//! Keep an existing startup preference, but point it at the installed build.
#[cfg(windows)]
pub fn update_existing() -> Result<(), String> {
    use winreg::{enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE}, RegKey};
    let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ | KEY_WRITE) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not update OpenMouse startup: {error}")),
    };
    update_key(&key, &crate::paths::app_exe_path())
}

#[cfg(windows)]
fn update_key(key: &winreg::RegKey, executable: &std::path::Path) -> Result<(), String> {
    match key.get_value::<String, _>("OpenMouse") {
        Ok(_) => key.set_value("OpenMouse", &format!("\"{}\" --autostart", executable.display()))
            .map_err(|error| format!("Could not update OpenMouse startup: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not read OpenMouse startup preference: {error}")),
    }
}

#[cfg(not(windows))]
pub fn update_existing() -> Result<(), String> { Ok(()) }

#[cfg(all(test, windows))]
mod tests {
    use super::update_key;
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    #[test]
    fn migrates_old_portable_path_without_enabling_disabled_startup() {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let test_path = format!(r"Software\OpenMouseInstallerTests\{}", std::process::id());
        let (key, _) = root.create_subkey(&test_path).unwrap();
        let installed = std::path::Path::new(r"C:\Users\Test User\Programs\OpenMouse Desktop\openmouse-gpui.exe");
        update_key(&key, installed).unwrap();
        assert!(key.get_value::<String, _>("OpenMouse").is_err());
        key.set_value("OtherApp", &"leave unchanged").unwrap();
        key.set_value("OpenMouse", &r#""C:\Downloads\old portable\OpenMouseDesktop.exe" --autostart"#).unwrap();
        update_key(&key, installed).unwrap();
        assert_eq!(key.get_value::<String, _>("OpenMouse").unwrap(), format!("\"{}\" --autostart", installed.display()));
        assert_eq!(key.get_value::<String, _>("OtherApp").unwrap(), "leave unchanged");
        drop(key);
        root.delete_subkey_all(&test_path).unwrap();
    }
}
