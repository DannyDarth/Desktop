//! Windows software macros. No device firmware or onboard profiles are written.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[cfg(windows)]
mod windows;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Key { code: u16, down: bool },
    Mouse { button: u8, down: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub delay_ms: u32,
    pub action: Action,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Trigger {
    Key {
        code: u16,
        #[serde(default)]
        modifiers: u8,
    },
    Shortcut {
        key: u16,
    },
    Mouse {
        button: u8,
    },
}
impl Trigger {
    pub fn same_input(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
    fn identity(&self) -> (u8, u16, u8) {
        match *self {
            Self::Key { code, modifiers } => (0, code, modifiers),
            Self::Shortcut { key } => (0, key, 5),
            Self::Mouse { button } => (1, u16::from(button), 0),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Key { code, modifiers } => {
                let mut parts = Vec::new();
                for (bit, name) in [(1, "Ctrl"), (2, "Shift"), (4, "Alt"), (8, "Win")] {
                    if modifiers & bit != 0 {
                        parts.push(name.to_string());
                    }
                }
                parts.push(key_name(*code));
                parts.join(" + ")
            }
            Self::Shortcut { key } => format!("Ctrl + Alt + {}", key_name(*key)),
            Self::Mouse { button } => {
                format!("Mouse {}", mouse_name(*button))
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MacroMode {
    NoRepeat,
    WhileHeld,
    Toggle,
    Sequence,
    #[default]
    Count,
}
impl MacroMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoRepeat => "No repeat",
            Self::WhileHeld => "Repeat while holding",
            Self::Toggle => "Toggle",
            Self::Sequence => "Sequence",
            Self::Count => "Repeat count",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    Press,
    #[default]
    Repeat,
    Release,
}
impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Press => "On press",
            Self::Repeat => "While held",
            Self::Release => "On release",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Macro {
    #[serde(default)]
    pub game_id: Option<String>,
    pub name: String,
    pub trigger: Trigger,
    pub repeats: u16,
    #[serde(default)]
    pub mode: MacroMode,
    #[serde(default)]
    pub on_press: Vec<Step>,
    #[serde(default)]
    pub on_release: Vec<Step>,
    pub steps: Vec<Step>,
}
impl Macro {
    pub fn section(&self, section: Section) -> &[Step] {
        match section {
            Section::Press => &self.on_press,
            Section::Repeat => &self.steps,
            Section::Release => &self.on_release,
        }
    }
    pub fn section_mut(&mut self, section: Section) -> &mut Vec<Step> {
        match section {
            Section::Press => &mut self.on_press,
            Section::Repeat => &mut self.steps,
            Section::Release => &mut self.on_release,
        }
    }
    pub fn event_count(&self) -> usize {
        if self.mode == MacroMode::Sequence {
            self.on_press.len() + self.steps.len() + self.on_release.len()
        } else {
            self.steps.len()
        }
    }
}
fn apply_order(steps: &[Step], held: &mut HashSet<u16>) -> Result<(), String> {
    for step in steps {
        let (key, down) = match step.action {
            Action::Key { code, down } => (code, down),
            Action::Mouse { button, down } => (0x100 + u16::from(button), down),
        };
        if down {
            if !held.insert(key) {
                return Err("A key is pressed twice without a release.".into());
            }
        } else if !held.remove(&key) {
            return Err("A release has no matching press.".into());
        }
    }
    Ok(())
}
fn validate_order(m: &Macro) -> Result<(), String> {
    let mut held = HashSet::new();
    if m.mode == MacroMode::Sequence {
        apply_order(&m.on_press, &mut held)?;
        let baseline = held.clone();
        apply_order(&m.steps, &mut held)?;
        if held != baseline {
            return Err(
                "The While held section must finish with the same keys held as it started.".into(),
            );
        }
        apply_order(&m.on_release, &mut held)?;
    } else {
        apply_order(&m.steps, &mut held)?;
    }
    if !held.is_empty() {
        return Err("Every press needs a matching release before playback.".into());
    }
    Ok(())
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Library {
    pub enabled: bool,
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub focus_only: HashSet<String>,
}
impl Library {
    /// The delay belongs to its event and travels with it. Playback validation
    /// rejects a release before its press or overlapping duplicate presses.
    pub fn move_step(&mut self, macro_index: usize, from: usize, to: usize) -> Result<(), String> {
        self.move_section_step(macro_index, Section::Repeat, from, to)
    }
    pub fn move_section_step(
        &mut self,
        macro_index: usize,
        section: Section,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        let mut next = self.clone();
        let m = next
            .macros
            .get_mut(macro_index)
            .ok_or("Macro no longer exists.")?;
        let steps = m.section_mut(section);
        if from >= steps.len() || to >= steps.len() {
            return Err("Step no longer exists.".into());
        }
        let step = steps.remove(from);
        steps.insert(to, step);
        next.validate_editing()
            .map_err(|e| format!("Cannot move this step: {e}"))?;
        *self = next;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_inner(true)
    }
    /// An unfinished order can be saved and reopened while playback remains blocked.
    pub fn validate_editing(&self) -> Result<(), String> {
        self.validate_inner(false)
    }
    pub fn playback_error(&self, index: usize) -> Option<String> {
        let Some(m) = self.macros.get(index) else {
            return Some("Macro no longer exists.".into());
        };
        Library {
            focus_only: HashSet::new(),
            enabled: false,
            macros: vec![m.clone()],
        }
        .validate()
        .err()
    }
    fn validate_inner(&self, check_order: bool) -> Result<(), String> {
        if self.macros.len() > 64 {
            return Err("At most 64 macros are supported.".into());
        }
        let mut triggers = Vec::new();
        for m in &self.macros {
            if m.name.is_empty()
                || m.name.chars().count() > 100
                || !(1..=100).contains(&m.repeats)
                || m.on_press.len() + m.steps.len() + m.on_release.len() > 1000
            {
                return Err("Invalid macro name, repeat count or step count.".into());
            }
            if triggers.contains(&(m.game_id.clone(), m.trigger.identity())) {
                return Err("Choose a different trigger; that trigger is already assigned.".into());
            }
            triggers.push((m.game_id.clone(), m.trigger.identity()));
            match m.trigger {
                Trigger::Key { code, modifiers }
                    if (recordable_key(code) || code == 0x77) && modifiers <= 15 => {}
                Trigger::Shortcut { key } if (0x70..=0x7b).contains(&key) && key != 0x77 => {}
                Trigger::Mouse { button: 1..=5 } => {}
                _ => return Err("Unsupported trigger.".into()),
            }
            let mut duration = 0u64;
            for step in m.on_press.iter().chain(&m.steps).chain(&m.on_release) {
                if step.delay_ms > 60_000 {
                    return Err("A delay cannot exceed 60 seconds.".into());
                }
                duration += u64::from(step.delay_ms);
                match step.action {
                    Action::Key { code, .. } if recordable_key(code) => {}
                    Action::Mouse { button: 1..=5, .. } => {}
                    _ => return Err("Unsupported input in macro.".into()),
                }
            }
            if check_order {
                validate_order(m)?;
            }
            if duration
                * if m.mode == MacroMode::Count {
                    u64::from(m.repeats)
                } else {
                    1
                }
                > 600_000
            {
                return Err("Playback is limited to ten minutes.".into());
            }
        }
        Ok(())
    }
}
pub fn parse_delay_ms(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Enter a whole number from 0 to 60000.".into());
    }
    text.parse::<u32>()
        .ok()
        .filter(|v| *v <= 60_000)
        .ok_or_else(|| "Enter a whole number from 0 to 60000.".into())
}
pub fn parse_macro_name(text: &str) -> Result<String, String> {
    let name = text.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("Use a name of 1 to 100 characters.".into());
    }
    Ok(name.to_string())
}
pub fn recordable_key(code: u16) -> bool {
    (0x08..=0xfe).contains(&code) && !matches!(code, 0x10..=0x12 | 0x1b | 0x77 | 0xe7)
}
pub fn key_name(code: u16) -> String {
    match code {
        0x41..=0x5a | 0x30..=0x39 => char::from_u32(u32::from(code)).unwrap().to_string(),
        0x70..=0x87 => format!("F{}", code - 0x6f),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0d => "Enter".into(),
        0x20 => "Space".into(),
        0xa0 => "Left Shift".into(),
        0xa1 => "Right Shift".into(),
        0xa2 => "Left Ctrl".into(),
        0xa3 => "Right Ctrl".into(),
        0xa4 => "Left Alt".into(),
        0xa5 => "Right Alt".into(),
        _ => format!("Key 0x{code:02X}"),
    }
}
pub fn mouse_name(button: u8) -> &'static str {
    match button {
        1 => "left",
        2 => "right",
        3 => "middle",
        4 => "Back",
        _ => "Forward",
    }
}
pub fn step_label(step: &Step) -> String {
    let (name, down) = match step.action {
        Action::Key { code, down } => (key_name(code), down),
        Action::Mouse { button, down } => (
            format!(
                "Mouse {}",
                match button {
                    1 => "left",
                    2 => "right",
                    3 => "middle",
                    4 => "Back",
                    _ => "Forward",
                }
            ),
            down,
        ),
    };
    format!("{} {name}", if down { "Press" } else { "Release" })
}
#[derive(Clone)]
pub struct Snapshot {
    pub library: Library,
    pub recording: Option<usize>,
    /// Unsaved input captured by the recorder; never replaces the saved macro until F8.
    pub recorded_steps: Vec<Step>,
    pub assigning: Option<usize>,
    pub section: Section,
    pub playing: bool,
    pub message: String,
    pub active_game: Option<String>,
    pub running_games: Vec<String>,
    pub focused_game: Option<String>,
}
pub(super) struct State {
    pub snapshot: Snapshot,
    pub capture: Option<(usize, std::time::Instant)>,
    pub draft: Vec<Step>,
    pub finish_requested: bool,
    pub trigger_capture: Option<(usize, std::time::Instant)>,
}
pub struct Engine {
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    path: Option<PathBuf>,
}
impl Engine {
    pub fn load() -> Self {
        let path = dirs::config_dir().map(|p| p.join("openmouse").join("macros.json"));
        let loaded = path.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| {
            serde_json::from_slice::<Library>(&b)
                .map_err(|e| e.to_string())
                .and_then(|l| l.validate_editing().map(|_| l))
        });
        let (library, message) = match loaded {
            Some(Ok(l)) => (l, String::new()),
            Some(Err(e)) => (Library::default(), format!("Could not load macros: {e}")),
            None => (Library::default(), String::new()),
        };
        Self {
            state: Arc::new(Mutex::new(State {
                snapshot: Snapshot {
                    library,
                    recording: None,
                    recorded_steps: Vec::new(),
                    assigning: None,
                    section: Section::Repeat,
                    playing: false,
                    message,
                    active_game: None,
                    running_games: Vec::new(),
                    focused_game: None,
                },
                capture: None,
                draft: Vec::new(),
                finish_requested: false,
                trigger_capture: None,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            path,
        }
    }
    pub fn start(&self, catalog: Vec<(String, Vec<String>)>) {
        #[cfg(not(windows))]
        let _ = &catalog;
        #[cfg(windows)]
        windows::start(
            Arc::downgrade(&self.state),
            self.cancel.clone(),
            self.path.clone(),
            catalog,
        );
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        let mut snapshot = state.snapshot.clone();
        snapshot.recorded_steps = if snapshot.recording.is_some() {
            state.draft.clone()
        } else {
            Vec::new()
        };
        snapshot
    }
    pub fn select_section(&self, section: Section) {
        let mut s = self.state.lock().unwrap();
        if !s.snapshot.playing && s.snapshot.recording.is_none() && s.snapshot.assigning.is_none() {
            s.snapshot.section = section;
        }
    }
    pub fn move_section_step(&self, index: usize, section: Section, from: usize, to: usize) {
        self.update(|library| library.move_section_step(index, section, from, to));
    }
    pub fn edit(&self, change: impl FnOnce(&mut Library)) {
        self.update(|library| {
            change(library);
            Ok(())
        });
    }
    pub fn move_step(&self, macro_index: usize, from: usize, to: usize) {
        self.update(|library| library.move_step(macro_index, from, to));
    }
    fn update(&self, change: impl FnOnce(&mut Library) -> Result<(), String>) {
        self.stop();
        let mut s = self.state.lock().unwrap();
        let mut next = s.snapshot.library.clone();
        let result = change(&mut next)
            .and_then(|_| next.validate_editing())
            .and_then(|_| self.save(&next));
        match result {
            Ok(()) => {
                s.snapshot.library = next;
                s.snapshot.message = "Saved".into();
            }
            Err(e) => s.snapshot.message = e,
        }
    }
    fn save(&self, library: &Library) -> Result<(), String> {
        persist(self.path.as_deref(), library)
    }
    pub fn sync_games(&self, running: Vec<String>) -> bool {
        let mut s = self.state.lock().unwrap();
        let next = choose_active_game(
            s.snapshot.active_game.as_deref(),
            &s.snapshot.running_games,
            &running,
            s.snapshot.focused_game.as_deref(),
            &s.snapshot.library.focus_only,
        );
        let changed = apply_active_game(&mut s, next, &self.cancel);
        s.snapshot.running_games = running;
        changed
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let mut s = self.state.lock().unwrap();
        s.capture = None;
        s.trigger_capture = None;
        if s.snapshot.assigning.take().is_some() {
            s.snapshot.message = "Input assignment cancelled; previous binding kept.".into();
        }
        s.draft.clear();
        s.finish_requested = false;
        if s.snapshot.recording.take().is_some() {
            s.snapshot.message = "Recording cancelled; saved steps kept.".into();
        }
    }
    pub fn assign_trigger(&self, index: usize) {
        self.stop();
        let mut s = self.state.lock().unwrap();
        if s.snapshot.playing || index >= s.snapshot.library.macros.len() {
            return;
        }
        s.snapshot.assigning = Some(index);
        s.trigger_capture = Some((
            index,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        ));
        s.snapshot.message =
            "Wait one second, then press a key, shortcut or mouse button. Escape cancels.".into();
    }
    pub fn record(&self, index: usize) {
        self.stop();
        let mut s = self.state.lock().unwrap();
        if s.snapshot.playing {
            s.snapshot.message = "Wait for playback to stop.".into();
            return;
        }
        if index >= s.snapshot.library.macros.len() {
            return;
        }
        s.snapshot.recording = Some(index);
        s.draft.clear();
        s.finish_requested = false;
        s.capture = Some((
            index,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        ));
        s.snapshot.message = "Recording begins in one second. F8 finishes; Escape cancels.".into();
    }
    pub fn finish_recording(&self) {
        self.state.lock().unwrap().finish_requested = true;
    }

    pub fn play(&self, index: usize) {
        #[cfg(windows)]
        windows::play(&self.state, &self.cancel, index, false);
        #[cfg(not(windows))]
        {
            let _ = index;
            self.state.lock().unwrap().snapshot.message = "Playback requires Windows.".into();
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

pub fn choose_active_game(
    current: Option<&str>,
    previous: &[String],
    running: &[String],
    focused: Option<&str>,
    focus_only: &HashSet<String>,
) -> Option<String> {
    if let Some(id) = focused.filter(|id| focus_only.contains(*id)) {
        return Some(id.to_string());
    }
    let eligible: Vec<_> = running
        .iter()
        .filter(|id| !focus_only.contains(*id))
        .cloned()
        .collect();
    choose_game(current, previous, &eligible)
}
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn apply_active_game(
    state: &mut State,
    next: Option<String>,
    cancel: &AtomicBool,
) -> bool {
    if state.snapshot.active_game == next {
        return false;
    }
    cancel.store(true, Ordering::SeqCst);
    state.capture = None;
    state.trigger_capture = None;
    state.snapshot.assigning = None;
    state.snapshot.recording = None;
    state.draft.clear();
    state.snapshot.active_game = next;
    true
}
/// The newest detected game takes priority; closing it restores another running game.
pub fn choose_game(
    current: Option<&str>,
    previous: &[String],
    running: &[String],
) -> Option<String> {
    running
        .iter()
        .rev()
        .find(|id| !previous.contains(id))
        .cloned()
        .or_else(|| {
            current
                .filter(|id| running.iter().any(|r| r == id))
                .map(str::to_owned)
        })
        .or_else(|| running.last().cloned())
}
fn persist(path: Option<&std::path::Path>, library: &Library) -> Result<(), String> {
    let path = path.ok_or("No configuration folder is available.")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    windows::replace_file(&temp, path)?;
    #[cfg(not(windows))]
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Whether another full cycle should run after the current one.
pub fn repeat_cycle(
    mode: MacroMode,
    triggered: bool,
    held: bool,
    pressed_again: bool,
    completed: u32,
    count: u16,
) -> bool {
    if !triggered {
        return mode == MacroMode::Count && completed < u32::from(count);
    }
    match mode {
        MacroMode::NoRepeat => false,
        MacroMode::Count => completed < u32::from(count),
        MacroMode::WhileHeld | MacroMode::Sequence => held,
        MacroMode::Toggle => !pressed_again,
    }
}
/// Translate an insertion gap (including the trailing gap) into the final index.
pub fn drop_target(from: usize, gap: usize, len: usize) -> Option<usize> {
    if from >= len || gap > len {
        return None;
    }
    let target = if gap > from { gap - 1 } else { gap };
    (target != from).then_some(target)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn library(steps: Vec<Step>) -> Library {
        Library {
            focus_only: HashSet::new(),
            enabled: false,
            macros: vec![Macro {
                mode: MacroMode::Count,
                on_press: vec![],
                on_release: vec![],
                game_id: None,
                name: "Test".into(),
                trigger: Trigger::Shortcut { key: 0x75 },
                repeats: 1,
                steps,
            }],
        }
    }
    fn key(down: bool) -> Step {
        Step {
            delay_ms: 10,
            action: Action::Key { code: 0x41, down },
        }
    }
    #[test]
    fn validates_balanced_input() {
        assert!(library(vec![key(true), key(false)]).validate().is_ok());
        assert!(library(vec![key(true)]).validate().is_err());
        assert!(library(vec![key(false)]).validate().is_err());
    }
    #[test]
    fn rejects_duplicate_triggers() {
        let mut l = library(vec![]);
        l.macros.push(l.macros[0].clone());
        assert!(l.validate().is_err());
    }
    #[test]
    fn rejects_oversized_delay_and_reserved_key() {
        let mut s = key(true);
        s.delay_ms = 60_001;
        assert!(library(vec![s, key(false)]).validate().is_err());
        assert!(!recordable_key(0x1b));
        assert!(!recordable_key(0x77));
    }
    #[test]
    fn limits_total_playback() {
        let mut l = library(vec![
            Step {
                delay_ms: 60_000,
                ..key(true)
            },
            key(false),
        ]);
        l.macros[0].repeats = 11;
        assert!(l.validate().is_err());
    }
    #[test]
    fn roundtrip_preserves_steps() {
        let l = library(vec![key(true), key(false)]);
        let decoded: Library = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(decoded.macros[0].steps[1].action, key(false).action);
    }
    #[test]
    fn game_switch_cancels_work_without_disabling_triggers() {
        let e = Engine {
            state: Arc::new(Mutex::new(State {
                snapshot: Snapshot {
                    library: Library {
                        focus_only: HashSet::new(),
                        enabled: true,
                        macros: vec![],
                    },
                    recording: Some(0),
                    recorded_steps: Vec::new(),
                    assigning: None,
                    section: Section::Repeat,
                    playing: true,
                    message: String::new(),
                    active_game: Some("one".into()),
                    running_games: vec!["one".into()],
                    focused_game: None,
                },
                capture: Some((0, std::time::Instant::now())),
                draft: vec![key(true)],
                finish_requested: false,
                trigger_capture: None,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            path: None,
        };
        let live = e.snapshot();
        assert_eq!(live.recorded_steps.len(), 1);
        assert_eq!(live.recorded_steps[0].action, key(true).action);
        assert!(live.library.macros.is_empty());
        assert!(e.sync_games(vec!["one".into(), "two".into()]));
        assert!(e.snapshot().recorded_steps.is_empty());
        assert!(e.cancel.load(Ordering::SeqCst));
        let s = e.state.lock().unwrap();
        assert_eq!(s.snapshot.active_game.as_deref(), Some("two"));
        assert!(s.snapshot.recording.is_none());
        assert!(s.capture.is_none());
        assert!(s.draft.is_empty());
        assert!(s.snapshot.library.enabled);
    }
    #[test]
    fn mouse_press_can_wrap_keyboard_steps_and_keep_its_delay() {
        let mut l = library(vec![
            key(true),
            key(false),
            Step {
                delay_ms: 25,
                action: Action::Mouse {
                    button: 1,
                    down: true,
                },
            },
            Step {
                delay_ms: 40,
                action: Action::Mouse {
                    button: 1,
                    down: false,
                },
            },
            Step {
                delay_ms: 10,
                action: Action::Key {
                    code: 0x42,
                    down: true,
                },
            },
            Step {
                delay_ms: 10,
                action: Action::Key {
                    code: 0x42,
                    down: false,
                },
            },
        ]);
        l.move_step(0, 2, 0).unwrap();
        l.move_step(0, 3, 5).unwrap();
        assert_eq!(
            l.macros[0].steps[0].action,
            Action::Mouse {
                button: 1,
                down: true
            }
        );
        assert_eq!(l.macros[0].steps[0].delay_ms, 25);
        assert_eq!(
            l.macros[0].steps[5].action,
            Action::Mouse {
                button: 1,
                down: false
            }
        );
        assert_eq!(l.macros[0].steps[5].delay_ms, 40);
        assert!(l.validate().is_ok());
    }
    #[test]
    fn invalid_move_keeps_the_original_sequence() {
        let mut l = library(vec![key(true), key(false)]);
        let before = serde_json::to_string(&l).unwrap();
        assert!(l.move_step(0, 0, 9).is_err());
        assert!(l.move_step(9, 0, 1).is_err());
        assert_eq!(serde_json::to_string(&l).unwrap(), before);
    }
    #[test]
    fn temporary_order_can_be_saved_and_repaired() {
        let mut l = library(vec![key(true), key(false)]);
        l.move_step(0, 1, 0).unwrap();
        assert!(l.validate_editing().is_ok());
        assert!(l.playback_error(0).is_some());
        let mut reopened: Library =
            serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        reopened.validate_editing().unwrap();
        reopened.move_step(0, 0, 1).unwrap();
        assert!(reopened.playback_error(0).is_none());
    }
    #[test]
    fn unfinished_macro_does_not_block_other_macros() {
        let mut l = library(vec![key(false), key(true)]);
        let mut valid = library(vec![key(true), key(false)]).macros.remove(0);
        valid.trigger = Trigger::Shortcut { key: 0x76 };
        l.macros.push(valid);
        assert!(l.playback_error(0).is_some());
        assert!(l.playback_error(1).is_none());
    }
    #[test]
    fn drag_gaps_support_first_last_and_ignore_noop_drops() {
        assert_eq!(drop_target(3, 0, 4), Some(0));
        assert_eq!(drop_target(0, 4, 4), Some(3));
        assert_eq!(drop_target(1, 1, 4), None);
        assert_eq!(drop_target(1, 2, 4), None);
        assert_eq!(drop_target(4, 0, 4), None);
        assert_eq!(drop_target(0, 5, 4), None);
    }
    #[test]
    fn supports_standard_keys_shortcuts_and_all_five_mouse_buttons() {
        for trigger in [
            Trigger::Key {
                code: 0x41,
                modifiers: 0,
            },
            Trigger::Key {
                code: 0x41,
                modifiers: 3,
            },
            Trigger::Key {
                code: 0x77,
                modifiers: 0,
            },
        ] {
            let mut l = library(vec![]);
            l.macros[0].trigger = trigger;
            assert!(l.validate().is_ok());
        }
        for button in 1..=5 {
            let mut l = library(vec![]);
            l.macros[0].trigger = Trigger::Mouse { button };
            assert!(l.validate().is_ok());
        }
        let mut l = library(vec![]);
        l.macros[0].trigger = Trigger::Key {
            code: 0x1b,
            modifiers: 0,
        };
        assert!(l.validate().is_err());
    }
    #[test]
    fn captured_shortcut_cannot_duplicate_a_legacy_binding() {
        let mut l = library(vec![]);
        let mut other = l.macros[0].clone();
        other.trigger = Trigger::Key {
            code: 0x75,
            modifiers: 5,
        };
        l.macros.push(other);
        assert!(l.validate().is_err());
    }
    #[test]
    fn all_four_types_have_distinct_repeat_behavior() {
        assert!(!repeat_cycle(MacroMode::NoRepeat, true, true, false, 1, 50));
        assert!(repeat_cycle(MacroMode::WhileHeld, true, true, false, 1, 1));
        assert!(!repeat_cycle(
            MacroMode::WhileHeld,
            true,
            false,
            false,
            1,
            1
        ));
        assert!(repeat_cycle(MacroMode::Toggle, true, false, false, 1000, 1));
        assert!(!repeat_cycle(MacroMode::Toggle, true, true, true, 1, 1));
        assert!(repeat_cycle(MacroMode::Sequence, true, true, false, 1, 1));
        assert!(!repeat_cycle(MacroMode::Sequence, true, false, false, 1, 1));
        for mode in [
            MacroMode::NoRepeat,
            MacroMode::WhileHeld,
            MacroMode::Toggle,
            MacroMode::Sequence,
        ] {
            assert!(!repeat_cycle(mode, false, true, false, 1, 1));
        }
    }
    #[test]
    fn sequence_can_hold_a_key_across_press_and_release() {
        let mut l = library(vec![]);
        let m = &mut l.macros[0];
        m.mode = MacroMode::Sequence;
        m.on_press = vec![key(true)];
        m.on_release = vec![key(false)];
        m.steps = vec![
            Step {
                delay_ms: 10,
                action: Action::Key {
                    code: 0x42,
                    down: true,
                },
            },
            Step {
                delay_ms: 10,
                action: Action::Key {
                    code: 0x42,
                    down: false,
                },
            },
        ];
        assert!(l.validate().is_ok());
        l.macros[0].steps.pop();
        assert!(l.validate().is_err());
        assert!(l.validate_editing().is_ok());
    }
    #[test]
    fn existing_recordings_keep_their_repeat_count() {
        let old = r#"{"enabled":false,"macros":[{"name":"Existing","trigger":{"kind":"shortcut","key":117},"repeats":3,"steps":[]}]}"#;
        let l: Library = serde_json::from_str(old).unwrap();
        assert_eq!(l.macros[0].mode, MacroMode::Count);
        assert_eq!(l.macros[0].repeats, 3);
        assert!(repeat_cycle(l.macros[0].mode, true, false, false, 2, 3));
        assert!(!repeat_cycle(l.macros[0].mode, true, false, false, 3, 3));
    }
    #[test]
    fn sequence_sections_survive_save_and_reorder_independently() {
        let mut l = library(vec![key(true), key(false)]);
        l.macros[0].mode = MacroMode::Sequence;
        l.macros[0].on_press = vec![key(true), key(false)];
        l.move_section_step(0, Section::Press, 0, 1).unwrap();
        assert_eq!(l.macros[0].steps[0].action, key(true).action);
        let reopened: Library = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(reopened.macros[0].mode, MacroMode::Sequence);
        assert_eq!(reopened.macros[0].on_press[0].action, key(false).action);
        assert!(reopened.playback_error(0).is_some());
    }
    #[test]
    fn focused_profile_follows_alt_tab_and_returns() {
        let only: HashSet<_> = ["game".to_string()].into_iter().collect();
        let running = vec!["game".to_string()];
        assert_eq!(
            choose_active_game(None, &running, &running, Some("game"), &only),
            Some("game".into())
        );
        assert_eq!(
            choose_active_game(Some("game"), &running, &running, None, &only),
            None
        );
        assert_eq!(
            choose_active_game(None, &running, &running, Some("game"), &only),
            Some("game".into())
        );
    }
    #[test]
    fn focused_game_has_priority_and_running_profiles_keep_their_behavior() {
        let only: HashSet<_> = ["focused".to_string()].into_iter().collect();
        let running = vec!["background".to_string(), "focused".to_string()];
        assert_eq!(
            choose_active_game(
                Some("background"),
                &running,
                &running,
                Some("focused"),
                &only
            ),
            Some("focused".into())
        );
        assert_eq!(
            choose_active_game(Some("focused"), &running, &running, None, &only),
            Some("background".into())
        );
        // Foreground detection can activate before the slower process scan catches up.
        assert_eq!(
            choose_active_game(None, &[], &[], Some("focused"), &only),
            Some("focused".into())
        );
    }
    #[test]
    fn focus_preferences_persist_per_game_without_changing_existing_defaults() {
        let old = r#"{"enabled":false,"macros":[]}"#;
        let mut l: Library = serde_json::from_str(old).unwrap();
        assert!(l.focus_only.is_empty());
        l.focus_only.insert("game".into());
        let reopened: Library = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert!(reopened.focus_only.contains("game"));
        assert!(!reopened.focus_only.contains("other"));
    }
    #[test]
    fn typed_delay_accepts_exact_values_and_rejects_invalid_input() {
        assert_eq!(parse_delay_ms(" 1963 "), Ok(1963));
        assert_eq!(parse_delay_ms("0"), Ok(0));
        assert_eq!(parse_delay_ms("60000"), Ok(60_000));
        for invalid in ["", "-1", "1.5", "1963 ms", "60001", "999999999999999"] {
            assert!(parse_delay_ms(invalid).is_err());
        }
    }
    #[test]
    fn macro_names_are_trimmed_and_cannot_be_blank_or_too_long() {
        assert_eq!(parse_macro_name("  Reload  "), Ok("Reload".into()));
        assert!(parse_macro_name("  ").is_err());
        assert!(parse_macro_name(&"x".repeat(101)).is_err());
        assert!(parse_macro_name(&"é".repeat(100)).is_ok());
    }
}
