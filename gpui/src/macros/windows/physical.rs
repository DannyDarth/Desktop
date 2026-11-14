//! Physical trigger state. Injected macro output never changes this state, so
//! holding and toggling work even when a macro emits its own activation key.
use super::super::State;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    OnceLock, Weak,
};
struct Inputs {
    keys: [AtomicBool; 256],
    presses: [AtomicU64; 256],
    bindings: [AtomicU64; 4096],
    ready: AtomicBool,
    foreground_clicks: AtomicU64,
}
static INPUTS: OnceLock<Inputs> = OnceLock::new();
fn inputs() -> &'static Inputs {
    INPUTS.get_or_init(|| Inputs {
        keys: std::array::from_fn(|_| AtomicBool::new(false)),
        presses: std::array::from_fn(|_| AtomicU64::new(0)),
        bindings: std::array::from_fn(|_| AtomicU64::new(0)),
        ready: AtomicBool::new(false),
        foreground_clicks: AtomicU64::new(0),
    })
}
fn update(code: usize, down: bool) {
    if code >= 256 {
        return;
    }
    let i = inputs();
    if down && !i.keys[code].swap(true, Ordering::SeqCst) {
        i.presses[code].fetch_add(1, Ordering::SeqCst);
        let mask = (if down_key(0xa2) || down_key(0xa3) {
            1
        } else {
            0
        }) | (if down_key(0xa0) || down_key(0xa1) {
            2
        } else {
            0
        }) | (if down_key(0xa4) || down_key(0xa5) {
            4
        } else {
            0
        }) | (if down_key(0x5b) || down_key(0x5c) {
            8
        } else {
            0
        });
        let own = match code {
            0xa2 | 0xa3 => 1,
            0xa0 | 0xa1 => 2,
            0xa4 | 0xa5 => 4,
            0x5b | 0x5c => 8,
            _ => 0,
        };
        i.bindings[code * 16 + (mask & !own)].fetch_add(1, Ordering::SeqCst);
    }
    if !down {
        i.keys[code].store(false, Ordering::SeqCst);
    }
}
#[repr(C)]
struct Keyboard {
    key: u32,
    scan: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
struct Mouse {
    point: Point,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Default)]
struct Message {
    window: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
    private: u32,
}
#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        id: i32,
        callback: unsafe extern "system" fn(i32, usize, isize) -> isize,
        module: isize,
        thread: u32,
    ) -> isize;
    fn CallNextHookEx(hook: isize, code: i32, wp: usize, lp: isize) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn PeekMessageW(msg: *mut Message, window: isize, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Message) -> i32;
    fn DispatchMessageW(msg: *const Message) -> isize;
    fn GetAsyncKeyState(key: i32) -> i16;
    fn GetForegroundWindow() -> isize;
    fn WindowFromPoint(point: Point) -> isize;
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}
fn record_key(event: &Keyboard, wp: usize) {
    if event.flags & 0x10 == 0 {
        let key = match event.key {
            0x10 => {
                if event.scan == 0x36 {
                    0xa1
                } else {
                    0xa0
                }
            }
            0x11 => {
                if event.flags & 1 != 0 {
                    0xa3
                } else {
                    0xa2
                }
            }
            0x12 => {
                if event.flags & 1 != 0 {
                    0xa5
                } else {
                    0xa4
                }
            }
            k => k,
        };
        if matches!(wp, 0x100 | 0x104 | 0x101 | 0x105) {
            update(key as usize, matches!(wp, 0x100 | 0x104));
        }
    }
}
unsafe extern "system" fn keyboard(code: i32, wp: usize, lp: isize) -> isize {
    if code >= 0 {
        record_key(unsafe { &*(lp as *const Keyboard) }, wp);
    }
    unsafe { CallNextHookEx(0, code, wp, lp) }
}
unsafe extern "system" fn mouse(code: i32, wp: usize, lp: isize) -> isize {
    if code >= 0 {
        let event = unsafe { &*(lp as *const Mouse) };
        if event.flags & 1 == 0 {
            let key = match wp {
                0x201 | 0x202 => Some(1),
                0x204 | 0x205 => Some(2),
                0x207 | 0x208 => Some(4),
                0x20b | 0x20c => Some(if event.data >> 16 == 1 { 5 } else { 6 }),
                _ => None,
            };
            if let Some(key) = key {
                if wp == 0x201 {
                    let hit = unsafe { GetAncestor(WindowFromPoint(event.point), 2) };
                    let foreground = unsafe { GetAncestor(GetForegroundWindow(), 2) };
                    if hit != 0 && hit == foreground {
                        inputs().foreground_clicks.fetch_add(1, Ordering::SeqCst);
                    }
                }
                update(key, matches!(wp, 0x201 | 0x204 | 0x207 | 0x20b));
            }
        }
    }
    unsafe { CallNextHookEx(0, code, wp, lp) }
}
pub(super) fn foreground_clicks() -> u64 {
    inputs().foreground_clicks.load(Ordering::SeqCst)
}
pub(super) fn ready() -> bool {
    inputs().ready.load(Ordering::SeqCst)
}
pub(super) fn down(key: u16) -> bool {
    let keys = &inputs().keys;
    match key {
        0x10 => keys[0xa0].load(Ordering::SeqCst) || keys[0xa1].load(Ordering::SeqCst),
        0x11 => keys[0xa2].load(Ordering::SeqCst) || keys[0xa3].load(Ordering::SeqCst),
        0x12 => keys[0xa4].load(Ordering::SeqCst) || keys[0xa5].load(Ordering::SeqCst),
        k => keys[usize::from(k)].load(Ordering::SeqCst),
    }
}
fn down_key(key: usize) -> bool {
    inputs().keys[key].load(Ordering::SeqCst)
}
pub(super) fn activations(key: u16, modifiers: Option<u8>) -> u64 {
    match modifiers {
        Some(mask) => {
            inputs().bindings[usize::from(key) * 16 + usize::from(mask)].load(Ordering::SeqCst)
        }
        None => inputs().presses[usize::from(key)].load(Ordering::SeqCst),
    }
}
pub(super) fn start(state: Weak<std::sync::Mutex<State>>) {
    std::thread::spawn(move || {
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let keyboard_hook = unsafe { SetWindowsHookExW(13, keyboard, module, 0) };
        let mouse_hook = unsafe { SetWindowsHookExW(14, mouse, module, 0) };
        if keyboard_hook == 0 || mouse_hook == 0 {
            unsafe {
                if keyboard_hook != 0 {
                    UnhookWindowsHookEx(keyboard_hook);
                }
                if mouse_hook != 0 {
                    UnhookWindowsHookEx(mouse_hook);
                }
            }
            if let Some(s) = state.upgrade() {
                s.lock().unwrap().snapshot.message =
                    "Physical input monitoring is unavailable. Hold/toggle modes cannot start."
                        .into();
            }
            return;
        }
        for key in 0..256 {
            inputs().keys[key].store(
                unsafe { GetAsyncKeyState(key as i32) < 0 },
                Ordering::SeqCst,
            );
        }
        inputs().ready.store(true, Ordering::SeqCst);
        while state.strong_count() > 0 {
            let mut message = Message::default();
            while unsafe { PeekMessageW(&mut message, 0, 0, 0, 1) } != 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        inputs().ready.store(false, Ordering::SeqCst);
        unsafe {
            UnhookWindowsHookEx(keyboard_hook);
            UnhookWindowsHookEx(mouse_hook);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_output_and_auto_repeat_do_not_count_as_new_presses() {
        let key = 0xf1;
        update(key, false);
        let baseline = activations(key as u16, None);
        let mut event = Keyboard {
            key: key as u32,
            scan: 0,
            flags: 0,
            time: 0,
            extra: 0,
        };
        record_key(&event, 0x100);
        record_key(&event, 0x100);
        assert_eq!(activations(key as u16, None), baseline + 1);
        event.flags = 0x10;
        record_key(&event, 0x101);
        assert!(down(key as u16));
        event.flags = 0;
        record_key(&event, 0x101);
        assert!(!down(key as u16));
        record_key(&event, 0x100);
        assert_eq!(activations(key as u16, None), baseline + 2);
        record_key(&event, 0x101);
    }
}
