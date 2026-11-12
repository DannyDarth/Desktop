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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub delay_ms: u32,
    pub action: Action,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Trigger {
    Shortcut { key: u16 },
    Mouse { button: u8 },
}
impl Trigger {
    pub fn label(&self) -> String {
        match self {
            Self::Shortcut { key } => format!("Ctrl + Alt + {}", key_name(*key)),
            Self::Mouse { button } => {
                format!("Mouse {}", if *button == 4 { "Back" } else { "Forward" })
            }
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
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Library {
    pub enabled: bool,
    pub macros: Vec<Macro>,
}
impl Library {
    pub fn validate(&self) -> Result<(), String> {
        if self.macros.len() > 64 {
            return Err("At most 64 macros are supported.".into());
        }
        let mut triggers = Vec::new();
        for m in &self.macros {
            if m.name.is_empty()
                || m.name.len() > 100
                || !(1..=100).contains(&m.repeats)
                || m.steps.len() > 1000
            {
                return Err("Invalid macro name, repeat count or step count.".into());
            }
            if triggers.contains(&(m.game_id.clone(), m.trigger.clone())) {
                return Err("Choose a different trigger; that trigger is already assigned.".into());
            }
            triggers.push((m.game_id.clone(), m.trigger.clone()));
            match m.trigger {
                Trigger::Shortcut { key } if (0x70..=0x7b).contains(&key) && key != 0x77 => {}
                Trigger::Mouse { button: 4 | 5 } => {}
                _ => return Err("Unsupported trigger.".into()),
            }
            let mut held = HashSet::new();
            let mut duration = 0u64;
            for step in &m.steps {
                if step.delay_ms > 60_000 {
                    return Err("A delay cannot exceed 60 seconds.".into());
                }
                duration += u64::from(step.delay_ms);
                let (key, down) = match step.action {
                    Action::Key { code, down } if recordable_key(code) => (code, down),
                    Action::Mouse { button, down } if (1..=5).contains(&button) => {
                        (0x100 + u16::from(button), down)
                    }
                    _ => return Err("Unsupported input in macro.".into()),
                };
                if down {
                    if !held.insert(key) {
                        return Err("A key is pressed twice without a release.".into());
                    }
                } else if !held.remove(&key) {
                    return Err("A release has no matching press.".into());
                }
            }
            if !held.is_empty() {
                return Err("Release every recorded key and button before saving.".into());
            }
            if duration * u64::from(m.repeats) > 600_000 {
                return Err("Playback is limited to ten minutes.".into());
            }
        }
        Ok(())
    }
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
    pub playing: bool,
    pub message: String,
    pub active_game: Option<String>,
    pub running_games: Vec<String>,
}
pub(super) struct State {
    pub snapshot: Snapshot,
    pub capture: Option<(usize, std::time::Instant)>,
    pub draft: Vec<Step>,
    pub finish_requested: bool,
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
                .and_then(|l| l.validate().map(|_| l))
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
                    playing: false,
                    message,
                    active_game: None,
                    running_games: Vec::new(),
                },
                capture: None,
                draft: Vec::new(),
                finish_requested: false,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            path,
        }
    }
    pub fn start(&self) {
        #[cfg(windows)]
        windows::start(
            Arc::downgrade(&self.state),
            self.cancel.clone(),
            self.path.clone(),
        );
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().snapshot.clone()
    }
    pub fn edit(&self, change: impl FnOnce(&mut Library)) {
        self.stop();
        let mut s = self.state.lock().unwrap();
        let mut next = s.snapshot.library.clone();
        change(&mut next);
        let result = next.validate().and_then(|_| self.save(&next));
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
        let next = choose_game(
            s.snapshot.active_game.as_deref(),
            &s.snapshot.running_games,
            &running,
        );
        let changed = next != s.snapshot.active_game;
        if changed {
            self.cancel.store(true, Ordering::SeqCst);
            s.capture = None;
            s.snapshot.recording = None;
            s.draft.clear();
            s.snapshot.active_game = next;
        }
        s.snapshot.running_games = running;
        changed
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let mut s = self.state.lock().unwrap();
        s.capture = None;
        s.draft.clear();
        s.finish_requested = false;
        if s.snapshot.recording.take().is_some() {
            s.snapshot.message = "Recording cancelled; saved steps kept.".into();
        }
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
        windows::play(&self.state, &self.cancel, index);
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

#[cfg(test)]
mod tests {
    use super::*;
    fn library(steps: Vec<Step>) -> Library {
        Library {
            enabled: false,
            macros: vec![Macro {
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
                        enabled: true,
                        macros: vec![],
                    },
                    recording: Some(0),
                    playing: true,
                    message: String::new(),
                    active_game: Some("one".into()),
                    running_games: vec!["one".into()],
                },
                capture: Some((0, std::time::Instant::now())),
                draft: vec![key(true)],
                finish_requested: false,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            path: None,
        };
        assert!(e.sync_games(vec!["one".into(), "two".into()]));
        assert!(e.cancel.load(Ordering::SeqCst));
        let s = e.state.lock().unwrap();
        assert_eq!(s.snapshot.active_game.as_deref(), Some("two"));
        assert!(s.snapshot.recording.is_none());
        assert!(s.capture.is_none());
        assert!(s.draft.is_empty());
        assert!(s.snapshot.library.enabled);
    }
}
