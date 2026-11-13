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
            discovered_profiles: Default::default(),
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

#[cfg(test)] mod discovery_defaults {
    use super::macros::Library;
    #[test] fn defaults_are_saved_once_and_later_user_choices_survive_relaunch() {
        let mut library=Library::default();
        library.register_discovered_profile("custom:new");
        assert!(library.focus_only.contains("custom:new"));
        library.focus_only.remove("custom:new");
        let mut restarted:Library=serde_json::from_str(&serde_json::to_string(&library).unwrap()).unwrap();
        restarted.register_discovered_profile("custom:new");
        assert!(!restarted.focus_only.contains("custom:new"));
        assert_eq!(restarted.discovered_profiles.len(),1);
    }
}

#[cfg(test)] mod discovery_aliases {
    use super::{games,macros};
    #[test] fn exact_path_aliases_reuse_the_bundled_profile_without_name_collisions() {
        let mut catalog=games::load_catalog();
        let game=catalog.iter_mut().find(|g|g.id=="counter-strike-2").unwrap();
        game.executables.push(r"D:\Steam\CS2\different-name.exe".into());
        let targets=games::macro_targets(&catalog);
        assert!(targets.iter().any(|g|g.id=="counter-strike-2" && g.matches(r"d:/steam/cs2/different-name.exe")));
        assert!(!targets.iter().any(|g|g.matches(r"D:\OtherApp\different-name.exe")));
        let matched=games::discovery::catalog_match(&catalog,r"d:/steam/cs2/different-name.exe").unwrap();
        assert_eq!(matched.id,"counter-strike-2");
        let mut library=macros::Library::default();library.register_discovered_profile(&matched.id);
        assert!(library.focus_only.contains("counter-strike-2"));
    }
}

#[path = "../../../src/profiles.rs"]
pub mod profiles;
