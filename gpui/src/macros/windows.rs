//! Native Windows input sampling and playback. The worker never samples injected
//! events during playback, and releases its held inputs on every exit path.
mod focus;
mod physical;
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
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(window: isize, process: *mut u32) -> u32;
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
    if physical::ready() && key != 0x1b {
        physical::down(key)
    } else {
        unsafe { GetAsyncKeyState(i32::from(key)) < 0 }
    }
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
        .filter(|c| {
            (matches!(c, 1 | 2 | 4 | 5 | 6) || recordable_key(*c) || *c == 0x77) && down(*c)
        })
        .collect()
}
fn modifier_bit(code: u16) -> u8 {
    match code {
        0xa2 | 0xa3 => 1,
        0xa0 | 0xa1 => 2,
        0xa4 | 0xa5 => 4,
        0x5b | 0x5c => 8,
        _ => 0,
    }
}
fn modifiers() -> u8 {
    [(0x11, 1), (0x10, 2), (0x12, 4), (0x5b, 8), (0x5c, 8)]
        .iter()
        .fold(
            0,
            |mask, (key, bit)| if down(*key) { mask | bit } else { mask },
        )
}
fn captured_trigger(code: u16) -> Trigger {
    match action(code, true) {
        Action::Mouse { button, .. } => Trigger::Mouse { button },
        _ => Trigger::Key {
            code,
            modifiers: modifiers() & !modifier_bit(code),
        },
    }
}
fn trigger_down(trigger: &Trigger) -> bool {
    match *trigger {
        Trigger::Key {
            code,
            modifiers: wanted,
        } => down(code) && (modifiers() & !modifier_bit(code)) == wanted,
        Trigger::Shortcut { key } => modifiers() == 5 && down(key),
        Trigger::Mouse { button } => down(match button {
            1 => 1,
            2 => 2,
            3 => 4,
            4 => 5,
            _ => 6,
        }),
    }
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
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
}
fn editor_is_foreground() -> bool {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == GetCurrentProcessId()
    }
}
fn activations(trigger: &Trigger) -> u64 {
    match *trigger {
        Trigger::Key { code, modifiers } => physical::activations(code, Some(modifiers)),
        Trigger::Shortcut { key } => physical::activations(key, Some(5)),
        Trigger::Mouse { button } => physical::activations(
            match button {
                1 => 1,
                2 => 2,
                3 => 4,
                4 => 5,
                _ => 6,
            },
            None,
        ),
    }
}
struct PlaybackControl {
    cancel: Arc<AtomicBool>,
    trigger: Trigger,
    mode: MacroMode,
    triggered: bool,
    initial: u64,
}
impl PlaybackControl {
    fn interrupted(&self, section: Section) -> bool {
        if is_cancelled(&self.cancel) {
            return true;
        }
        if !self.triggered {
            return false;
        }
        match self.mode {
            MacroMode::Toggle => activations(&self.trigger) > self.initial,
            MacroMode::WhileHeld => !trigger_down(&self.trigger),
            MacroMode::Sequence if section == Section::Repeat => !trigger_down(&self.trigger),
            _ => false,
        }
    }
}
fn run_steps(
    steps: &[Step],
    section: Section,
    control: &PlaybackControl,
    held: &mut Vec<Action>,
) -> Result<bool, String> {
    for step in steps {
        let end = Instant::now() + Duration::from_millis(u64::from(step.delay_ms));
        while Instant::now() < end {
            if control.interrupted(section) {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if control.interrupted(section) {
            return Ok(false);
        }
        send(&step.action)?;
        if pressed(&step.action) {
            held.push(step.action.clone());
        } else {
            held.retain(|a| released(a) != step.action);
        }
    }
    Ok(true)
}
pub(super) fn play(
    state: &Arc<Mutex<State>>,
    cancel: &Arc<AtomicBool>,
    index: usize,
    triggered: bool,
) {
    let mut s = state.lock().unwrap();
    if s.snapshot.playing || s.snapshot.recording.is_some() || s.snapshot.assigning.is_some() {
        return;
    }
    let Some(m) = s.snapshot.library.macros.get(index).cloned() else {
        return;
    };
    if m.event_count() == 0 || m.game_id != s.snapshot.active_game {
        s.snapshot.message = "This macro is empty or not in the active game set.".into();
        return;
    }
    if let Some(e) = s.snapshot.library.playback_error(index) {
        s.snapshot.message = e;
        return;
    }
    if triggered
        && !physical::ready()
        && matches!(
            m.mode,
            MacroMode::Toggle | MacroMode::WhileHeld | MacroMode::Sequence
        )
    {
        s.snapshot.message = "Physical input monitoring is unavailable. Restart OpenMouse before using this macro type.".into();
        return;
    }
    s.snapshot.playing = true;
    s.snapshot.message = format!("Playing {}. Escape stops playback.", m.name);
    cancel.store(false, Ordering::SeqCst);
    let control = PlaybackControl {
        cancel: cancel.clone(),
        trigger: m.trigger.clone(),
        mode: m.mode,
        triggered,
        initial: activations(&m.trigger),
    };
    let state = state.clone();
    std::thread::spawn(move || {
        let mut held: Vec<Action> = Vec::new();
        let outcome = (|| -> Result<(), String> {
            if !triggered {
                // The preview button must be released before injecting input.
                let deadline = Instant::now() + Duration::from_secs(5);
                while !sample().is_empty()
                    && !is_cancelled(&control.cancel)
                    && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                if !sample().is_empty() {
                    return Ok(());
                }
            }
            if m.mode == MacroMode::Sequence {
                if !run_steps(&m.on_press, Section::Press, &control, &mut held)? {
                    return Ok(());
                }
                if !triggered {
                    run_steps(&m.steps, Section::Repeat, &control, &mut held)?;
                } else {
                    while trigger_down(&m.trigger) && !is_cancelled(&control.cancel) {
                        let baseline = held.clone();
                        if !run_steps(&m.steps, Section::Repeat, &control, &mut held)? {
                            if !is_cancelled(&control.cancel) {
                                let extras: Vec<_> = held
                                    .iter()
                                    .filter(|a| !baseline.contains(a))
                                    .cloned()
                                    .collect();
                                for a in extras {
                                    send(&released(&a))?;
                                    held.retain(|h| *h != a);
                                }
                                for a in baseline {
                                    if !held.contains(&a) {
                                        send(&a)?;
                                        held.push(a);
                                    }
                                }
                            }
                            break;
                        }
                        if !wait(8, &control.cancel) {
                            break;
                        }
                    }
                }
                if !is_cancelled(&control.cancel) {
                    run_steps(&m.on_release, Section::Release, &control, &mut held)?;
                }
            } else {
                let mut cycles = 0;
                loop {
                    if !run_steps(&m.steps, Section::Repeat, &control, &mut held)? {
                        break;
                    }
                    cycles += 1;
                    if !repeat_cycle(
                        m.mode,
                        triggered,
                        trigger_down(&m.trigger),
                        activations(&m.trigger) > control.initial,
                        cycles,
                        m.repeats,
                    ) {
                        break;
                    }
                    // A zero-delay recording must not spin at unlimited speed.
                    if !wait(8, &control.cancel) {
                        break;
                    }
                }
            }
            Ok(())
        })();
        for a in held.iter().rev() {
            let _ = send(&released(a));
        }
        let mut s = state.lock().unwrap();
        s.snapshot.playing = false;
        s.snapshot.message = outcome.err().unwrap_or_else(|| {
            if is_cancelled(&control.cancel) {
                "Playback stopped".into()
            } else {
                "Playback finished".into()
            }
        });
    });
}
pub(super) fn start(
    weak: Weak<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    path: Option<PathBuf>,
    catalog: Arc<Mutex<Vec<GameTarget>>>,
) {
    focus::start(weak.clone(), cancel.clone(), catalog);
    physical::start(weak.clone());
    std::thread::spawn(move || {
        let mut previous = sample();
        let mut last = Instant::now();
        let mut recording = None;
        let mut held: HashSet<u16> = HashSet::new();
        let mut triggers = HashSet::new();
        let mut after_playback = false;
        let mut assigning = None;
        let mut pending_modifier = None;
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
                s.trigger_capture = None;
                s.snapshot.assigning = None;
                s.snapshot.recording = None;
                s.draft.clear();
                s.snapshot.message = "Stopped by Escape".into();
                recording = None;
                previous = now;
                continue;
            }
            if let Some((index, ready)) = s.trigger_capture {
                if Instant::now() < ready {
                    previous = now;
                    continue;
                }
                if assigning != Some((index, ready)) {
                    assigning = Some((index, ready));
                    pending_modifier = None;
                    previous = now.clone();
                }
                let mut pressed: Vec<_> = now.difference(&previous).copied().collect();
                pressed.sort_unstable();
                let mut chosen = None;
                for code in pressed {
                    if modifier_bit(code) != 0 {
                        pending_modifier = Some(code);
                    } else {
                        chosen = Some(captured_trigger(code));
                        break;
                    }
                }
                if chosen.is_none() {
                    if let Some(code) = pending_modifier {
                        if !now.contains(&code) {
                            chosen = Some(Trigger::Key { code, modifiers: 0 });
                        }
                    }
                }
                if let Some(trigger) = chosen {
                    let mut next = s.snapshot.library.clone();
                    if let Some(m) = next.macros.get_mut(index) {
                        m.trigger = trigger.clone();
                    }
                    match next
                        .validate_editing()
                        .and_then(|_| persist(path.as_deref(), &next))
                    {
                        Ok(()) => {
                            s.snapshot.library = next;
                            s.snapshot.message = format!("Assigned {}", trigger.label());
                        }
                        Err(e) => s.snapshot.message = e,
                    }
                    s.trigger_capture = None;
                    s.snapshot.assigning = None;
                    assigning = None;
                    // Wait for the assigned input to be released before arming it.
                    after_playback = true;
                }
                previous = now;
                continue;
            }
            assigning = None;
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
                        let section = if m.mode == MacroMode::Sequence {
                            s.snapshot.section
                        } else {
                            Section::Repeat
                        };
                        *m.section_mut(section) = draft;
                    }
                    match next
                        .validate_editing()
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
                if s.snapshot.library.enabled && !editor_is_foreground() {
                    for (i, m) in s
                        .snapshot
                        .library
                        .macros
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.game_id == s.snapshot.active_game)
                    {
                        let is_down = trigger_down(&m.trigger);
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
                    play(&state, &cancel, index, true);
                    previous = now;
                    continue;
                }
            }
            previous = now;
        }
    });
}
