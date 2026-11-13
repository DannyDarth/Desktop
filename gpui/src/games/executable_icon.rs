//! Extract a Windows executable's associated icon without launching the game.
use std::path::{Path, PathBuf};
pub fn cache_path(dir: &Path, executable: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    executable.hash(&mut hash);
    if let Ok(meta) = std::fs::metadata(executable) {
        meta.len().hash(&mut hash);
        meta.modified().ok().hash(&mut hash);
    }
    dir.join(format!("exe-icon-{:016x}.png", hash.finish()))
}
#[cfg(windows)]
pub fn extract(executable: &str, target: &Path) -> bool {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if target.is_file() {
        return true;
    }
    let Some(parent) = target.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let temporary = target.with_extension("tmp.png");
    // Paths are environment values, never interpolated into PowerShell source.
    const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$icon = $null; $bitmap = $null
try {
  $source = $env:OPENMOUSE_ICON_SOURCE
  if ($source.StartsWith('\\?\UNC\')) { $source = '\\' + $source.Substring(8) }
  elseif ($source.StartsWith('\\?\')) { $source = $source.Substring(4) }
  $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($source)
  if ($null -eq $icon) { exit 1 }
  $bitmap = $icon.ToBitmap()
  $bitmap.Save($env:OPENMOUSE_ICON_TARGET, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
  if ($null -ne $bitmap) { $bitmap.Dispose() }
  if ($null -ne $icon) { $icon.Dispose() }
}"#;
    let shell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let child = Command::new(shell)
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("OPENMOUSE_ICON_SOURCE", executable)
        .env("OPENMOUSE_ICON_TARGET", &temporary)
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break false;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let png = success
        && std::fs::read(&temporary).is_ok_and(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    if png && std::fs::rename(&temporary, target).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(temporary);
    false
}
#[cfg(not(windows))]
pub fn extract(_executable: &str, _target: &Path) -> bool {
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_uses_a_windows_safe_filename_even_for_personal_game_paths() {
        let a = cache_path(Path::new("cache"), r"C:\Games\Helldivers 2\helldivers2.exe");
        let b = cache_path(Path::new("cache"), r"D:\Games\Helldivers 2\helldivers2.exe");
        assert_ne!(a, b);
        assert!(!a.file_name().unwrap().to_string_lossy().contains(':'));
        assert_eq!(a.extension().unwrap(), "png");
    }
    #[cfg(windows)]
    #[test]
    fn windows_extracts_an_actual_executable_icon() {
        let Some(root) = std::env::var_os("SystemRoot") else {
            return;
        };
        let source = PathBuf::from(root).join("System32").join("notepad.exe");
        if !source.is_file() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("openmouse-icon-test-{}", std::process::id()));
        let target = dir.join("icon.png");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(extract(&source.to_string_lossy(), &target));
        assert!(std::fs::read(&target)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
