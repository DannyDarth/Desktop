//! Fast foreground checks are independent of the slower installed/running scan.
use super::super::{apply_active_game, choose_active_game, GameTarget, State};
use std::sync::{atomic::AtomicBool, Arc, Mutex, Weak};
#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(window: isize, process: *mut u32) -> u32;
}
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, process: u32) -> isize;
    fn QueryFullProcessImageNameW(
        process: isize,
        flags: u32,
        path: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}
fn foreground_executable() -> Option<String> {
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
    if ok == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    Some(path)
}
pub(super) fn start(
    weak: Weak<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    catalog: Arc<Mutex<Vec<GameTarget>>>,
) {
    std::thread::spawn(move || {
        while let Some(state) = weak.upgrade() {
            let executable = foreground_executable();
            let focused = executable.as_deref().and_then(|path| {
                let catalog = catalog.lock().unwrap();
                catalog
                    .iter()
                    .filter(|g| g.path.is_some())
                    .chain(catalog.iter().filter(|g| g.path.is_none()))
                    .find(|g| g.matches(path))
                    .map(|g| g.id.clone())
            });
            {
                let mut s = state.lock().unwrap();
                s.snapshot.focused_game = focused;
                let next = choose_active_game(
                    s.snapshot.active_game.as_deref(),
                    &s.snapshot.running_games,
                    &s.snapshot.running_games,
                    s.snapshot.focused_game.as_deref(),
                    &s.snapshot.library.focus_only,
                );
                apply_active_game(&mut s, next, &cancel);
            }
            drop(state);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}
