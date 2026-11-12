//! Native Windows input sampling and playback. The worker never samples injected
//! events during playback, and releases its held inputs on every exit path.
use super::*;
use std::{
    collections::HashSet,
    sync::Weak,
    time::{Duration, Instant},
};
#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct KeyInput {
    key: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
union InputData {
    mouse: MouseInput,
    key: KeyInput,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    data: InputData,
}
#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn MapVirtualKeyW(code: u32, kind: u32) -> u32;
}
#[link(name = "kernel32")]
extern "system" {
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
}
pub(super) fn replace_file(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
fn down(key: u16) -> bool {
    unsafe { GetAsyncKeyState(i32::from(key)) < 0 }
}
fn action(code: u16, pressed: bool) -> Action {
    match code {
        1 => Action::Mouse {
            button: 1,
            down: pressed,
        },
        2 => Action::Mouse {
            button: 2,
            down: pressed,
        },
        4 => Action::Mouse {
            button: 3,
            down: pressed,
        },
        5 => Action::Mouse {
            button: 4,
            down: pressed,
        },
        6 => Action::Mouse {
            button: 5,
            down: pressed,
        },
        _ => Action::Key {
            code,
            down: pressed,
        },
    }
}
fn sample() -> HashSet<u16> {
    (1..=254)
        .filter(|c| (matches!(c, 1 | 2 | 4 | 5 | 6) || recordable_key(*c)) && down(*c))
        .collect()
}
fn send(action: &Action) -> Result<(), String> {
    let input = match *action {
        Action::Key { code, down } => {
            let scan = unsafe { MapVirtualKeyW(u32::from(code), 4) };
            Input {
                kind: 1,
                data: InputData {
                    key: KeyInput {
                        key: if scan == 0 { code } else { 0 },
                        scan: (scan & 0xff) as u16,
                        flags: (if down { 0 } else { 2 })
                            | (if scan != 0 { 8 } else { 0 })
                            | (if scan >> 8 == 0xe0 { 1 } else { 0 }),
                        time: 0,
                        extra: 0,
                    },
                },
            }
        }
        Action::Mouse { button, down } => {
            let (flags, data) = match button {
                1 => (if down { 2 } else { 4 }, 0),
                2 => (if down { 8 } else { 16 }, 0),
                3 => (if down { 32 } else { 64 }, 0),
                4 => (if down { 128 } else { 256 }, 1),
                _ => (if down { 128 } else { 256 }, 2),
            };
            Input {
                kind: 0,
                data: InputData {
                    mouse: MouseInput {
                        dx: 0,
                        dy: 0,
                        data,
                        flags,
                        time: 0,
                        extra: 0,
                    },
                },
            }
        }
    };
    if unsafe { SendInput(1, &input, std::mem::size_of::<Input>() as i32) } != 1 {
        return Err(
            "Windows rejected input. Check whether the target runs as administrator.".into(),
        );
    }
    Ok(())
}
fn released(action: &Action) -> Action {
    match *action {
        Action::Key { code, .. } => Action::Key { code, down: false },
        Action::Mouse { button, .. } => Action::Mouse {
            button,
            down: false,
        },
    }
}
fn pressed(action: &Action) -> bool {
    match *action {
        Action::Key { down, .. } | Action::Mouse { down, .. } => down,
    }
}
fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::SeqCst) || down(0x1b)
}
fn wait(ms: u32, cancel: &AtomicBool) -> bool {
    let end = Instant::now() + Duration::from_millis(u64::from(ms));
    while Instant::now() < end {
        if is_cancelled(cancel) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    !is_cancelled(cancel)
}
pub(super) fn play(state: &Arc<Mutex<State>>, cancel: &Arc<AtomicBool>, index: usize) {
    let mut s = state.lock().unwrap();
    if s.snapshot.playing || s.snapshot.recording.is_some() {
        return;
    }
    let Some(m) = s.snapshot.library.macros.get(index).cloned() else {
        return;
    };
    if m.steps.is_empty() || m.game_id != s.snapshot.active_game {
        s.snapshot.message = "This macro is not in the active game set.".into();
        return;
    }
    if let Some(e) = s.snapshot.library.playback_error(index) {
        s.snapshot.message = e;
        return;
    }
    s.snapshot.playing = true;
    s.snapshot.message = format!("Playing {}. Escape stops playback.", m.name);
    cancel.store(false, Ordering::SeqCst);
    let state = state.clone();
    let cancel = cancel.clone();
    std::thread::spawn(move || {
        let mut held: Vec<Action> = Vec::new();
        let mut error = None;
        // Let the triggering shortcut and mouse click finish before injecting.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sample().is_empty() && !is_cancelled(&cancel) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(4));
        }
        if !sample().is_empty() {
            cancel.store(true, Ordering::SeqCst);
        }
        'playback: for _ in 0..m.repeats {
            for step in &m.steps {
                if !wait(step.delay_ms, &cancel) {
                    break 'playback;
                }
                if let Err(e) = send(&step.action) {
                    error = Some(e);
                    break 'playback;
                }
                if pressed(&step.action) {
                    held.push(step.action.clone());
                } else {
                    held.retain(|a| released(a) != step.action);
                }
            }
        }
        for a in held.iter().rev() {
            let _ = send(&released(a));
        }
        let mut s = state.lock().unwrap();
        s.snapshot.playing = false;
        s.snapshot.message = error.unwrap_or_else(|| {
            if is_cancelled(&cancel) {
                "Playback stopped".into()
            } else {
                "Playback finished".into()
            }
        });
    });
}
pub(super) fn start(weak: Weak<Mutex<State>>, cancel: Arc<AtomicBool>, path: Option<PathBuf>) {
    std::thread::spawn(move || {
        let mut previous = sample();
        let mut last = Instant::now();
        let mut recording = None;
        let mut held: HashSet<u16> = HashSet::new();
        let mut triggers = HashSet::new();
        let mut after_playback = false;
        loop {
            std::thread::sleep(Duration::from_millis(8));
            let Some(state) = weak.upgrade() else {
                break;
            };
            let mut s = state.lock().unwrap();
            if s.snapshot.playing {
                after_playback = true;
                previous = sample();
                triggers.clear();
                continue;
            }
            let now = sample();
            if after_playback {
                previous = now.clone();
                triggers.clear();
                if now.is_empty() {
                    after_playback = false;
                }
                continue;
            }
            if down(0x1b) {
                cancel.store(true, Ordering::SeqCst);
                s.capture = None;
                s.snapshot.recording = None;
                s.draft.clear();
                s.snapshot.message = "Stopped by Escape".into();
                recording = None;
                previous = now;
                continue;
            }
            if let Some((index, ready)) = s.capture {
                if Instant::now() < ready {
                    previous = now;
                    continue;
                }
                if recording != Some((index, ready)) {
                    recording = Some((index, ready));
                    held.clear();
                    previous = now.clone();
                    last = Instant::now();
                    s.snapshot.message = "Recording. F8 saves; Escape cancels.".into();
                }
                let finish = down(0x77) || s.finish_requested || s.draft.len() + held.len() >= 998;
                if finish {
                    let mut keys: Vec<_> = held.drain().collect();
                    keys.sort_unstable();
                    for code in keys {
                        s.draft.push(Step {
                            delay_ms: 0,
                            action: action(code, false),
                        });
                    }
                    let draft = std::mem::take(&mut s.draft);
                    let mut next = s.snapshot.library.clone();
                    if let Some(m) = next.macros.get_mut(index) {
                        m.steps = draft;
                    }
                    match next
                        .validate_editing()
                        .and_then(|_| next.playback_error(index).map_or(Ok(()), Err))
                        .and_then(|_| persist(path.as_deref(), &next))
                    {
                        Ok(()) => {
                            s.snapshot.library = next;
                            s.snapshot.message = "Recording saved".into();
                        }
                        Err(e) => s.snapshot.message = e,
                    }
                    s.capture = None;
                    s.snapshot.recording = None;
                    s.finish_requested = false;
                    recording = None;
                } else {
                    // Sort transitions to keep recordings deterministic. Release events go first.
                    let mut changes: Vec<_> =
                        previous.symmetric_difference(&now).copied().collect();
                    changes.sort_unstable();
                    for code in changes {
                        let is_down = now.contains(&code);
                        if is_down {
                            if s.draft.len() + held.len() + 2 > 1000 {
                                s.finish_requested = true;
                                continue;
                            }
                            held.insert(code);
                        } else if !held.remove(&code) {
                            continue;
                        }
                        let delay_ms = last.elapsed().as_millis().min(60_000) as u32;
                        last = Instant::now();
                        s.draft.push(Step {
                            delay_ms,
                            action: action(code, is_down),
                        });
                    }
                }
            } else {
                recording = None;
                let mut active = HashSet::new();
                let mut chosen = None;
                if s.snapshot.library.enabled {
                    for (i, m) in s
                        .snapshot
                        .library
                        .macros
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.game_id == s.snapshot.active_game)
                    {
                        let is_down = match m.trigger {
                            Trigger::Shortcut { key } => down(0x11) && down(0x12) && down(key),
                            Trigger::Mouse { button } => down(if button == 4 { 5 } else { 6 }),
                        };
                        if is_down {
                            active.insert(i);
                            if !triggers.contains(&i) && chosen.is_none() {
                                chosen = Some(i);
                            }
                        }
                    }
                }
                triggers = active;
                if let Some(index) = chosen {
                    drop(s);
                    play(&state, &cancel, index);
                    previous = now;
                    continue;
                }
            }
            previous = now;
        }
    });
}
