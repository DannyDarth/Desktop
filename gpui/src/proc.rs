//! Child-process helpers.

use std::process::Command;

/// Keeps a console window from flashing up on Windows when this GUI app starts a
/// command-line tool (`curl`). No effect elsewhere.
pub fn quiet(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Ignore user curl configuration and bound every network transfer. On Windows
/// use the OS tool rather than searching the app directory or PATH.
pub fn curl() -> Command {
    #[cfg(windows)]
    let executable = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join(r"System32\curl.exe");
    #[cfg(not(windows))]
    let executable = std::path::PathBuf::from("curl");
    let mut command = Command::new(executable);
    command.args(["-q", "--proto", "=https", "--proto-redir", "=https",
        "--max-redirs", "3", "--max-filesize", "8388608", "--connect-timeout", "5", "--max-time", "20"]);
    command
}

#[cfg(test)]
mod security_tests {
    #[test]
    fn network_helper_rejects_local_file_urls() {
        let file = std::env::temp_dir().join(format!("openmouse-network-test-{}.txt", std::process::id()));
        std::fs::write(&file, b"private local data").unwrap();
        let output = super::curl().arg(format!("file:///{}", file.to_string_lossy().replace('\\', "/")))
            .stdin(std::process::Stdio::null()).output().unwrap();
        std::fs::remove_file(file).unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
