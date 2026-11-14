use super::*;

#[test]
fn malformed_function_names_do_not_become_supported_shortcuts() {
    for raw in [
        "f0", "f13", "f255", "f256", "f01", "f001", "f+1", "f-1", "f1.0", "win+f01",
    ] {
        assert!(Trigger::parse(raw).is_none(), "{raw}");
    }
    assert_eq!(
        Trigger::parse("f"),
        Some(Trigger::Key(KeySpec {
            mods: Mods::NONE,
            id: KeyId::Char("f".into())
        }))
    );
}
#[test]
fn existing_function_key_bindings_survive_map_projection_and_conflict_checks() {
    for number in 1..=12 {
        let raw = format!("f{number}");
        let trigger = Trigger::parse(&raw).expect("existing  F1-F12 binding");
        let map = InputMap::from_bindings([
            (InputAction::NavLeft, raw.clone()),
            (InputAction::NavRight, raw),
        ]);
        assert_eq!(map.triggers(InputAction::NavLeft), &[trigger]);
        assert!(map
            .conflicts()
            .contains(&(InputAction::NavLeft, InputAction::NavRight)));
    }
}
#[test]
fn existing_named_key_aliases_round_trip_without_default_substitution() {
    for raw in [
        "escape",
        "esc",
        "delete",
        "del",
        "pgup",
        "page_up",
        "pgdn",
        "page_down",
    ] {
        let trigger = Trigger::parse(raw).expect("existing  named key alias");
        assert_eq!(Trigger::parse(&trigger.config()), Some(trigger.clone()));
        let map = InputMap::from_bindings([(InputAction::NavLeft, raw.into())]);
        assert_eq!(map.triggers(InputAction::NavLeft), &[trigger]);
    }
}
#[test]
fn existing_logo_and_hyper_modifiers_keep_their_full_identity() {
    for raw in ["win+f10", "super+left", "windows+delete", "hyper+f12"] {
        let trigger = Trigger::parse(raw).expect("existing  logo modifier");
        assert_eq!(Trigger::parse(&trigger.config()), Some(trigger.clone()));
        let map = InputMap::from_bindings([(InputAction::NavLeft, raw.into())]);
        assert_eq!(map.triggers(InputAction::NavLeft), &[trigger]);
    }
    assert_ne!(Trigger::parse("f10"), Trigger::parse("win+f10"));
    assert_eq!(
        Trigger::parse("hyper+f12"),
        Trigger::parse("ctrl+alt+shift+win+f12")
    );
}
