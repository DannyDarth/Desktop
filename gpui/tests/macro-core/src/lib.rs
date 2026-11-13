// Exercises the real engine without linking the desktop GUI or requiring HID hardware.
#[path = "../../../src/macros/mod.rs"]
pub mod macros;
#[cfg(test)]
mod tests {
    use super::macros::*;
    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn game_lifecycle_switches_and_restores() {
        assert_eq!(choose_game(None, &[], &ids(&["one"])), Some("one".into()));
        assert_eq!(
            choose_game(Some("one"), &ids(&["one"]), &ids(&["one", "two"])),
            Some("two".into())
        );
        assert_eq!(
            choose_game(Some("two"), &ids(&["one", "two"]), &ids(&["one"])),
            Some("one".into())
        );
        assert_eq!(choose_game(Some("one"), &ids(&["one"]), &[]), None);
    }
    #[test]
    fn retains_active_game_on_unchanged_scan() {
        assert_eq!(
            choose_game(Some("one"), &ids(&["one", "two"]), &ids(&["one", "two"])),
            Some("one".into())
        );
    }
    #[test]
    fn same_trigger_allowed_in_different_games() {
        let m = Macro {
            mode: MacroMode::Count,
            on_press: vec![],
            on_release: vec![],
            game_id: Some("one".into()),
            name: "Test".into(),
            trigger: Trigger::Shortcut { key: 0x75 },
            repeats: 1,
            steps: vec![],
        };
        let mut second = m.clone();
        second.game_id = Some("two".into());
        let l = Library {
            focus_only: Default::default(),
            enabled: true,
            macros: vec![m, second],
        };
        assert!(l.validate().is_ok());
    }
}




#[path = "../../../src/games.rs"]
pub mod games;
#[path = "../../../src/proc.rs"]
pub mod proc;
