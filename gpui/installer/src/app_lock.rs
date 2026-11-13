pub(crate) fn ensure_file_available(path: &std::path::Path) -> Result<(), String> {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Close OpenMouse Desktop from its tray icon, then try again. The installed app could not be updated or removed: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_file_available;
    #[test]
    fn availability_check_preserves_existing_files_and_allows_first_install() {
        let dir = std::env::temp_dir().join(format!("openmouse-setup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.exe");
        std::fs::write(&file, b"existing app").unwrap();
        ensure_file_available(&file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"existing app");
        ensure_file_available(&dir.join("missing.exe")).unwrap();
        assert!(ensure_file_available(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn locked_executable_blocks_changes() {
        use std::os::windows::fs::OpenOptionsExt;
        let file = std::env::temp_dir().join(format!("openmouse-locked-{}.exe", std::process::id()));
        std::fs::write(&file, b"existing app").unwrap();
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
        assert!(ensure_file_available(&file).is_err());
        drop(held);
        ensure_file_available(&file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"existing app");
        std::fs::remove_file(file).unwrap();
    }

}

