pub(crate) const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Wait-Process -Id ([int]$env:OPENMOUSE_REMOVE_PID) -ErrorAction SilentlyContinue
$dir = $env:OPENMOUSE_REMOVE_DIR
foreach ($name in @($env:OPENMOUSE_REMOVE_APP_NAME, 'openmouse-uninstaller.exe')) {
    $file = Join-Path $dir $name
    if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
}
if ((Get-ChildItem -LiteralPath $dir -Force | Measure-Object).Count -eq 0) {
    Remove-Item -LiteralPath $dir -Force
}
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_treats_special_paths_literally_and_preserves_extra_files() {
        let dir = std::env::temp_dir().join(format!("openmouse-cleanup-'&%[test]-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::payload::APP_EXE_NAME), b"app").unwrap();
        std::fs::write(dir.join("openmouse-uninstaller.exe"), b"setup").unwrap();
        std::fs::write(dir.join("keep.txt"), b"user file").unwrap();
        let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let run = || std::process::Command::new(&shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", super::SCRIPT])
            .env("OPENMOUSE_REMOVE_DIR", &dir)
            .env("OPENMOUSE_REMOVE_APP_NAME", crate::payload::APP_EXE_NAME)
            .env("OPENMOUSE_REMOVE_PID", "2147483647")
            .output().unwrap();
        let output = run();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!dir.join(crate::payload::APP_EXE_NAME).exists());
        assert!(!dir.join("openmouse-uninstaller.exe").exists());
        assert_eq!(std::fs::read(dir.join("keep.txt")).unwrap(), b"user file");
        std::fs::remove_file(dir.join("keep.txt")).unwrap();
        assert!(run().status.success());
        assert!(!dir.exists());
    }
}
