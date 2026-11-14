use super::*;

fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("w3-v4-{name}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root.join("candidate-state.toml")
}

#[test]
fn w3_v3_read_only_then_raw_archive_and_unknown_roundtrip() {
    let path = fixture("raw");
    let raw = b"# exact legacy bytes\r\nversion = 3\r\nfavorites = ['kept']\r\n[extra_root]\r\nfuture = [1, 2]\r\n[frontend_preferences.future]\r\nintent = 'inactive'\r\n";
    fs::write(&path, raw).unwrap();
    let loaded = CandidateState::load_with_diagnostics(&path);
    assert_eq!(fs::read(&path).unwrap(), raw);
    assert!(!path.with_extension("toml.bak").exists());
    assert!(loaded.diagnostic.unwrap().contains("inactive"));
    let mut state = loaded.state;
    state.local_library.edit("stable", " Local ", true).unwrap();
    state
        .local_library
        .retained
        .insert("future".into(), toml::Value::String("inert".into()));
    state.save(&path).unwrap();
    let saved = CandidateState::load(&path);
    assert_eq!(saved.version, 4);
    assert_eq!(saved.retained, state.retained);
    assert_eq!(
        saved.frontend_preferences.retained,
        state.frontend_preferences.retained
    );
    assert_eq!(saved.local_library, state.local_library);
    assert!(saved.favorites.contains("kept"));
    let files = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("v3-to-v4-primary") && fs::read(p).unwrap() == raw));
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("absent-to-v4-backup")));
}

#[test]
fn w3_local_tombstone_normalization_orphan_reload_and_bounds() {
    let path = fixture("tags");
    let mut state = CandidateState::default();
    let source = BTreeSet::from(["source".into(), "keep".into()]);
    state.local_library.edit("id", " SOURCE ", false).unwrap();
    state.local_library.edit("id", " Added ", true).unwrap();
    assert_eq!(
        state.local_library.effective_tags("id", &source),
        BTreeSet::from(["keep".into(), "added".into()])
    );
    assert!(state
        .local_library
        .edit("id", &"x".repeat(129), true)
        .is_err());
    let before = state.clone();
    state.save(&path).unwrap();
    let mut loaded = CandidateState::load(&path);
    assert_eq!(loaded.local_library, before.local_library);
    assert!(loaded.local_library.tag_overrides.contains_key("id")); // no catalog is required
    loaded.local_library.edit("id", "source", true).unwrap();
    assert!(loaded
        .local_library
        .effective_tags("id", &source)
        .contains("source"));
    loaded
        .retained
        .insert("oversize".into(), toml::Value::String("x".repeat(65537)));
    assert!(loaded.save(&path).is_err());
    assert_eq!(
        CandidateState::load(&path).local_library,
        before.local_library
    );
}

#[test]
fn w3_future_primary_or_backup_refuses_without_loss() {
    for backup in [false, true] {
        let path = fixture(if backup {
            "future-backup"
        } else {
            "future-primary"
        });
        let current = toml::to_string(&CandidateState::default()).unwrap();
        let future = b"version=5\nnew='untouched'\n";
        fs::write(&path, if backup { current.as_bytes() } else { future }).unwrap();
        fs::write(
            path.with_extension("toml.bak"),
            if backup { future } else { current.as_bytes() },
        )
        .unwrap();
        let raw = fs::read(&path).unwrap();
        let bak = fs::read(path.with_extension("toml.bak")).unwrap();
        let loaded = CandidateState::load_with_diagnostics(&path);
        assert!(loaded.state.write_restriction.is_some());
        assert!(loaded.state.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), raw);
        assert_eq!(fs::read(path.with_extension("toml.bak")).unwrap(), bak);
    }
}
