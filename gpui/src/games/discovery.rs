//! Foreground-only discovery avoids treating crash handlers and installers as games.
use super::{custom, install_scan, Game};
use std::path::Path;
#[derive(Debug)]
pub enum Discovered {
    Existing(String),
    Added(custom::CustomGame),
}
pub fn catalog_match<'a>(catalog: &'a [Game], executable: &str) -> Option<&'a Game> {
    let name = executable.rsplit(['\\', '/']).next()?;
    catalog
        .iter()
        .filter(|g| g.executable_path.is_some())
        .chain(catalog.iter().filter(|g| g.executable_path.is_none()))
        .find(|g| {
            g.executable_path
                .as_ref()
                .is_some_and(|path| crate::macros::paths_match(path, executable))
                || g.executables.iter().any(|e| {
                    if e.contains(['\\', '/']) {
                        crate::macros::paths_match(e, executable)
                    } else {
                        g.executable_path.is_none() && e.eq_ignore_ascii_case(name)
                    }
                })
        })
}
pub fn eligible(executable: &str) -> bool {
    let name = executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".exe")
        && ![
            "launcher",
            "crash",
            "uninstall",
            "setup",
            "redist",
            "anticheat",
            "easyanticheat",
            "battleye",
            "updater",
            "helper",
        ]
        .iter()
        .any(|s| name.contains(s))
}
pub fn discover(executable: &str) -> Result<Option<Discovered>, String> {
    let catalog = super::load_catalog();
    if let Some(game) = catalog_match(&catalog, executable) {
        // Manual profiles keep their activation preferences.
        if game.executable_path.is_some()
            && !custom::load()?
                .iter()
                .any(|c| c.id == game.id && c.auto_discovered)
        {
            return Ok(None);
        }
        return Ok(Some(Discovered::Existing(game.id.clone())));
    }
    if !eligible(executable) {
        return Ok(None);
    }
    let (steam, name) =
        if let Some((id, name)) = install_scan::steam_title_for_executable(executable) {
            (Some(id), name)
        } else if let Some(name) = install_scan::epic_title_for_executable(executable) {
            (None, name)
        } else {
            return Ok(None);
        };
    let game = custom::add_discovered(Path::new(executable), &name, steam)?;
    if !game.auto_discovered {
        return Ok(None);
    }
    Ok(Some(Discovered::Added(game)))
}
#[cfg(not(windows))]
pub fn foreground_executable() -> Option<String> {
    None
}
#[cfg(windows)]
pub fn foreground_executable() -> Option<String> {
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(window: isize, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn QueryFullProcessImageNameW(
            handle: isize,
            flags: u32,
            path: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
    }
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if handle == 0 {
        return None;
    }
    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(handle);
    }
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..size as usize]))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_launchers_helpers_and_ordinary_unrecognized_paths() {
        assert!(eligible(r"D:\Steam\steamapps\common\Game\bin\Game.exe"));
        for exe in [
            "Launcher.exe",
            "CrashReport.exe",
            "EasyAntiCheat.exe",
            "setup.exe",
            "Updater.exe",
            "readme.txt",
        ] {
            assert!(!eligible(exe), "{exe}");
        }
        assert!(discover(r"C:\Unknown Folder\Unrecognized Game.exe")
            .unwrap()
            .is_none());
    }
}
