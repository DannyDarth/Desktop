//! Windows can leave the game's foreground HWND unchanged while Start opens.
//! Latch a physical Windows-key departure until focus returns or the user
//! dismisses the shell / clicks back into the game. Never resume on search typing.
use std::time::{Duration, Instant};
#[derive(Default)]
pub(crate) struct DesktopFocusGate {
    paused: bool,
    saw_away: bool,
    settle: Option<Instant>,
    resume: Option<Instant>,
}
impl DesktopFocusGate {
    pub(crate) fn game_focused(
        &mut self,
        game: bool,
        windows_pressed: bool,
        windows_held: bool,
        resume_requested: bool,
        now: Instant,
    ) -> bool {
        if windows_pressed {
            if !self.paused && game {
                self.paused = true;
                self.saw_away = false;
                self.settle = Some(now + Duration::from_millis(250));
                self.resume = None;
            } else if self.paused {
                self.resume = Some(now + Duration::from_millis(250));
            }
        }
        if self.paused {
            if !game {
                self.saw_away = true;
            }
            if resume_requested {
                self.resume = Some(now + Duration::from_millis(250));
            }
            let settled = self.settle.is_none_or(|when| now >= when);
            if game
                && !windows_held
                && settled
                && (self.saw_away || self.resume.is_some_and(|when| now >= when))
            {
                self.paused = false;
                self.saw_away = false;
                self.settle = None;
                self.resume = None;
            }
        }
        game && !self.paused && !windows_held
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_menu_does_not_reactivate_the_game_when_its_hwnd_is_unchanged() {
        let now = Instant::now();
        let mut gate = DesktopFocusGate::default();
        assert!(gate.game_focused(true, false, false, false, now));
        assert!(!gate.game_focused(true, true, true, false, now));
        assert!(!gate.game_focused(true, false, false, false, now + Duration::from_secs(5)));
        assert!(!gate.game_focused(true, false, false, true, now + Duration::from_secs(6)));
        assert!(gate.game_focused(true, false, false, false, now + Duration::from_millis(6300)));
    }
    #[test]
    fn genuine_focus_departure_and_return_resumes_the_profile() {
        let now = Instant::now();
        let mut gate = DesktopFocusGate::default();
        gate.game_focused(true, true, true, false, now);
        assert!(!gate.game_focused(false, false, false, false, now + Duration::from_millis(300)));
        assert!(gate.game_focused(true, false, false, false, now + Duration::from_secs(1)));
    }
    #[test]
    fn second_windows_press_can_close_start_without_a_foreground_change() {
        let now = Instant::now();
        let mut gate = DesktopFocusGate::default();
        gate.game_focused(true, true, true, false, now);
        gate.game_focused(true, true, true, false, now + Duration::from_secs(1));
        assert!(gate.game_focused(true, false, false, false, now + Duration::from_millis(1300)));
    }
}

#[cfg(test)]
mod profile_policy_tests {
    use super::*;
    #[test]
    fn windows_key_restores_default_only_for_focus_based_profiles() {
        let mut gate = DesktopFocusGate::default();
        let focused = gate.game_focused(true, true, true, false, Instant::now());
        assert!(!focused);
        let running = vec!["game".to_owned()];
        let focus_only = std::collections::HashSet::from(["game".to_owned()]);
        assert_eq!(
            crate::macros::choose_active_game(Some("game"), &running, &running, None, &focus_only),
            None
        );
        assert_eq!(
            crate::macros::choose_active_game(
                Some("game"),
                &running,
                &running,
                None,
                &Default::default()
            ),
            Some("game".into())
        );
    }
}
