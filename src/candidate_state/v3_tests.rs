use super::*;

#[test]
fn p4_raw_archive_labels_classify_typed_malformed_and_future_counterparts() {
    for (bytes, expected) in [
        (None, "absent"),
        (Some(b"favorites=[]".to_vec()), "v0"),
        (Some(b"version=2\nfavorites=[]".to_vec()), "v2"),
        (Some(b"version=2\nfavorites=42".to_vec()), "malformed"),
        (Some(b"unrecognized=true".to_vec()), "malformed"),
        (Some(b"version=5\nfavorites=42".to_vec()), "v5"),
    ] {
        assert_eq!(raw_version_label(&bytes), expected);
    }
}

#[test]
fn p4_validation_failure_reports_preserved_backup_without_touching_raw_pair() {
    let root = std::env::temp_dir().join(format!("p4-validation-backup-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let backup = path.with_extension("toml.bak");
    let mut state = CandidateState::default();
    persist_candidate_state(&mut state, &path).unwrap();
    persist_candidate_state(&mut state, &path).unwrap();
    let before = (fs::read(&path).unwrap(), fs::read(&backup).unwrap());
    let mut proposed = state.clone();
    proposed.card_flip_mut().duration_ms = 0;
    let error = commit_saved_state(&mut state, proposed, &path).unwrap_err();
    assert_eq!(error.stage, SaveStage::PreparePrimary);
    assert_eq!(error.backup, BackupStatus::Preserved);
    assert_eq!(
        (fs::read(&path).unwrap(), fs::read(&backup).unwrap()),
        before
    );
    assert!(state.reconciliation_required.is_none());
}

#[test]
fn p4_legacy_preset_integer_payloads_above_float_precision_remain_exact() {
    let root = std::env::temp_dir().join(format!(
        "p4-legacy-integer-precision-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let raw=b"version=2\n[[gallery.presets]]\nname='large legacy values'\nmode='depth'\n[gallery.presets.depth]\nvisible_count=16777217\nnavigation_ms=16777217\n";
    fs::write(&path, raw).unwrap();
    let mut state = CandidateState::load(&path);
    for _ in 0..2 {
        let original = state.gallery.presets[0].depth.as_ref().unwrap();
        let preserved = state.mode_profiles.depth.entries[0]
            .legacy_snapshot
            .as_ref()
            .unwrap()
            .depth
            .as_ref()
            .unwrap();
        assert_eq!(original.visible_count, 16_777_217_usize);
        assert_eq!(original.navigation_ms, 16_777_217_u64);
        assert_eq!(preserved, original);
        let active = state.mode_profiles.depth.entries[0]
            .snapshot
            .as_ref()
            .unwrap();
        assert_eq!((active.visible_count, active.navigation_ms), (21.0, 8000.0));
        persist_candidate_state(&mut state, &path).unwrap();
        state = CandidateState::load(&path);
    }
}

#[cfg(test)]
mod p4_migration_tests {
    use super::*;

    #[test]
    fn p4_v2_depth_survives_the_production_v3_save_and_restart() {
        let root = std::env::temp_dir().join(format!("p4-v2-depth-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("candidate-state.toml");
        let raw = b"# owned legacy v2\nversion=2\n[gallery]\nmode='depth'\n[gallery.depth]\nwidth=317.0\nspacing=23.0\nvisible_count=7\nnavigation_ms=1456\n[gallery.depth.detail]\neffect='Tonal Layers'\nduration_ms=2300\nanimate_shader=false\nanimate_back=false\n[[gallery.presets]]\nname='duplicate'\nmode='depth'\n[gallery.presets.depth]\nwidth=411.0\n[[gallery.presets]]\nname='duplicate'\nmode='depth'\n";
        fs::write(&path, raw).unwrap();
        let mut loaded = CandidateState::load(&path);
        assert_eq!(fs::read(&path).unwrap(), raw, "load must be read-only");
        assert_eq!(
            loaded.version, CURRENT_STATE_VERSION,
            "current production version"
        );
        assert_eq!(loaded.gallery.depth.width, 317.0);
        assert_eq!(loaded.gallery.depth.navigation_ms, 1456.0);
        assert_eq!(
            loaded.gallery.presets[0].depth.as_ref().unwrap().width,
            411.0
        );
        assert!(loaded.gallery.presets[1].depth.is_none());
        let before = loaded.gallery.clone();
        let outcome = persist_candidate_state(&mut loaded, &path).unwrap();
        assert_eq!(outcome.migration.len(), 2);
        assert_eq!(fs::read(&outcome.migration[0].snapshot).unwrap(), raw);
        assert!(outcome.migration[0]
            .snapshot
            .to_string_lossy()
            .contains(&format!("migration-v2-to-v{CURRENT_STATE_VERSION}-primary")));
        assert!(outcome.migration[1]
            .snapshot
            .to_string_lossy()
            .contains(&format!(
                "migration-absent-to-v{CURRENT_STATE_VERSION}-backup"
            )));
        assert_eq!(CandidateState::load(&path).gallery, before);
        assert!(loaded.pending_migration.is_none());
    }

    #[test]
    fn p4_archive_labels_preserve_mixed_actual_input_versions_and_history() {
        let root = std::env::temp_dir().join(format!("p4-mixed-labels-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        let primary_raw = b"# v2\nversion=2\nfavorites=['two']";
        let backup_raw = b"# v1\nversion=1\nfavorites=['one']";
        let history = root.join("candidate-state.toml.migration-v1-to-v2-primary-historical.raw");
        let complete = history.with_extension("raw.complete");
        fs::write(&history, b"historical raw bytes").unwrap();
        fs::write(&complete, b"historical completion bytes").unwrap();
        fs::write(&path, primary_raw).unwrap();
        fs::write(&backup, backup_raw).unwrap();
        let mut loaded = CandidateState::load(&path);
        let outcome = persist_candidate_state(&mut loaded, &path).unwrap();
        assert_eq!(outcome.migration.len(), 2);
        assert!(outcome.migration[0]
            .snapshot
            .to_string_lossy()
            .contains(&format!("migration-v2-to-v{CURRENT_STATE_VERSION}-primary")));
        assert!(outcome.migration[1]
            .snapshot
            .to_string_lossy()
            .contains(&format!("migration-v1-to-v{CURRENT_STATE_VERSION}-backup")));
        assert_eq!(
            fs::read(&outcome.migration[0].snapshot).unwrap(),
            primary_raw
        );
        assert_eq!(
            fs::read(&outcome.migration[1].snapshot).unwrap(),
            backup_raw
        );
        assert_eq!(fs::read(&history).unwrap(), b"historical raw bytes");
        assert_eq!(fs::read(&complete).unwrap(), b"historical completion bytes");
    }
}
#[test]
fn p4_fixed_nord_and_legacy_omitted_applied_snapshot_have_distinct_defaults() {
    use crate::theme_roles::{ColorVariant, ThemeRole};
    let fresh = CandidateState::default();
    let nord = frontend::nord_document();
    assert_eq!(fresh.applied_theme.as_ref().unwrap().document, nord);
    assert_eq!(
        format!(
            "{:X}",
            Sha256::digest(include_bytes!("../../assets/themes/nord.json"))
        ),
        "81FCAF4EAF2C4D61101823236A6F7EAA9409BBE8204EDC8130A4B93089AE25D1"
    );
    for role in ThemeRole::ALL {
        for variant in [ColorVariant::Dark, ColorVariant::Light] {
            assert!(!nord.color(role, variant).is_empty());
        }
    }
    for version in [0, 1, 2, 3] {
        let legacy = parse_state_contents(&format!(
            "version={version}\n[appearance]\npreset='custom'\naccent='#123456'\n"
        ))
        .unwrap();
        assert!(legacy.applied_theme.is_none());
        let encoded = toml::to_string_pretty(&legacy).unwrap();
        assert!(!encoded.contains("[applied_theme]"));
        let restarted = parse_state_contents(&encoded).unwrap();
        assert!(restarted.applied_theme.is_none());
        assert_eq!(restarted.appearance, legacy.appearance);
    }
}

#[test]
fn p4_shared_flip_imports_once_and_explicit_record_survives_all_mode_reloads() {
    let root = std::env::temp_dir().join(format!("p4-shared-flip-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    fs::write(&path, b"version=2\n[gallery.depth.detail]\neffect='Pixel Sort'\nduration_ms=2300\nanimate_shader=false\nanimate_back=false\n").unwrap();
    let mut state = CandidateState::load(&path);
    assert_eq!(state.card_flip(), &state.gallery.depth.detail);
    state.card_flip.as_mut().unwrap().duration_ms = 987;
    let shared = state.card_flip.clone();
    let legacy = state.gallery.depth.detail.clone();
    for mode in [
        GalleryKind::Slices,
        GalleryKind::Geometric,
        GalleryKind::Wall,
        GalleryKind::Depth,
    ] {
        state.gallery.mode = mode;
        persist_candidate_state(&mut state, &path).unwrap();
        state = CandidateState::load(&path);
        assert_eq!(state.gallery.mode, mode);
        assert_eq!(state.card_flip, shared);
        assert_eq!(state.gallery.depth.detail, legacy);
    }
}

#[test]
fn p4_legacy_current_detail_shadow_survives_shared_seed_and_explicit_precedence() {
    for (version, explicit) in [(2, false), (3, false), (3, true)] {
        let root = std::env::temp_dir().join(format!(
            "p4-legacy-detail-shadow-{}-{version}-{explicit}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        let shared = if explicit {
            "\n[card_flip]\neffect='Pixel Sort'\nduration_ms=987\nanimate_shader=true\nanimate_back=true\n"
        } else {
            ""
        };
        let original = format!("version={version}\n[gallery.depth.detail]\neffect='future-legacy-effect'\nduration_ms=99999\nanimate_shader=false\nanimate_back=false\n{shared}");
        fs::write(&path, original.as_bytes()).unwrap();
        let mut state = CandidateState::load(&path);
        assert_eq!(
            fs::read(&path).unwrap(),
            original.as_bytes(),
            "Load is read-only"
        );
        assert!(!path.with_extension("toml.bak").exists());
        let shadow = state.gallery.depth.detail.clone();
        assert_eq!(
            shadow.effect, "future-legacy-effect",
            "Unused legacy storage is not active shared settings"
        );
        assert_eq!(shadow.duration_ms, 99999);
        assert!(!shadow.animate_shader && !shadow.animate_back);
        let effective = state.card_flip().clone();
        assert_eq!(
            effective.effect,
            if explicit { "Pixel Sort" } else { "Ignite" }
        );
        assert_eq!(effective.duration_ms, if explicit { 987 } else { 5000 });
        assert_eq!(effective.animate_shader, explicit);
        assert_eq!(effective.animate_back, explicit);
        for _ in 0..3 {
            persist_candidate_state(&mut state, &path).unwrap();
            state = CandidateState::load(&path);
            assert_eq!(state.gallery.depth.detail, shadow);
            assert_eq!(
                state.card_flip(),
                &effective,
                "Explicit shared values never reseed from the retained shadow"
            );
        }
    }
}

#[test]
fn p4_library_save_and_active_delete_publish_independent_applied_snapshots() {
    use crate::theme_roles::{ColorVariant, ThemeRole};
    let root = std::env::temp_dir().join(format!("p4-theme-library-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let mut state = CandidateState::default();
    let original =
        frontend::nord_document().replaced(ThemeRole::ALL[0], ColorVariant::Dark, "#123456");
    let id = state
        .theme_library
        .save(None, "Nord", original.clone())
        .unwrap();
    state.applied_theme = Some(AppliedThemeSnapshot {
        origin: ThemeOrigin::Custom { record_id: id },
        variant: ColorVariant::Dark,
        document: original.clone(),
    });
    persist_candidate_state(&mut state, &path).unwrap();
    let mut proposed = state.clone();
    let changed = original.replaced(ThemeRole::ALL[1], ColorVariant::Light, "#abcdef");
    proposed
        .theme_library
        .save(Some(id), "Nord", changed.clone())
        .unwrap();
    commit_saved_state(&mut state, proposed, &path).unwrap();
    state = CandidateState::load(&path);
    assert_eq!(state.theme_library.records[0].document, changed);
    assert_eq!(state.applied_theme.as_ref().unwrap().document, original);
    let delete = state.propose_theme_delete(id).unwrap();
    assert_eq!(
        state.theme_library.records.len(),
        1,
        "proposal must not install itself"
    );
    assert_eq!(delete.applied_theme, Some(AppliedThemeSnapshot::default()));
    let blocked = root.join("blocked.toml");
    fs::create_dir(&blocked).unwrap();
    assert!(commit_saved_state(&mut state, delete.clone(), &blocked).is_err());
    assert_eq!(state.theme_library.records.len(), 1);
    assert_eq!(state.applied_theme.as_ref().unwrap().document, original);
    commit_saved_state(&mut state, delete, &path).unwrap();
    let restarted = CandidateState::load(&path);
    assert!(restarted.theme_library.records.is_empty());
    assert_eq!(
        restarted.applied_theme,
        Some(AppliedThemeSnapshot::default())
    );
}

#[test]
fn p4_legacy_payloads_remain_exact_while_typed_projection_is_safe() {
    let root = std::env::temp_dir().join(format!("p4-legacy-payload-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let raw = "version=2\n[[gallery.presets]]\nname='duplicate'\nmode='slices'\nvariant=2\n[gallery.presets.slices]\nwidth=0.0\ncorner_radius=12.0\n[gallery.presets.geometric]\nradius=-1.0\nshape='circle'\n[gallery.presets.wall]\ncard_width=0.0\n[gallery.presets.depth]\nwidth=1.0\n[[gallery.presets]]\nname='duplicate'\nmode='geometric'\n[gallery.presets.geometric]\nshape='circle'\n[[gallery.presets]]\nname='duplicate'\nmode='wall'\n[[gallery.presets]]\nname='duplicate'\nmode='depth'\n[[gallery.presets]]\nname='duplicate'\nmode='depth'\n[gallery.presets.depth]\nwidth=411.0\n";
    let original: CandidateState = toml::from_str(raw).unwrap();
    fs::write(&path, raw).unwrap();
    let mut state = CandidateState::load(&path);
    assert_eq!(state.gallery.presets, original.gallery.presets);
    let profiles = &state.mode_profiles;
    assert_eq!(profiles.next_id, 6);
    assert_eq!(
        profiles.slices.entries[0].legacy_snapshot.as_ref(),
        Some(&original.gallery.presets[0])
    );
    let projected = profiles.slices.entries[0].snapshot.as_ref().unwrap();
    assert_eq!(projected.width, 1.0);
    assert!(projected.round_corners);
    assert_eq!(projected.corners, [12.0; 4]);
    assert_eq!(
        profiles.hex.entries[0].legacy_snapshot.as_ref(),
        Some(&original.gallery.presets[1])
    );
    assert_eq!(
        profiles.hex.entries[0].snapshot.as_ref().unwrap().shape,
        GeometricShape::Circle
    );
    assert_eq!(
        profiles.wall.entries[0].legacy_snapshot.as_ref(),
        Some(&original.gallery.presets[2])
    );
    assert!(profiles.depth.entries[0].snapshot.is_none());
    assert_eq!(
        profiles.depth.entries[0].legacy_snapshot.as_ref(),
        Some(&original.gallery.presets[3])
    );
    assert_eq!(
        profiles.depth.entries[1].legacy_snapshot.as_ref(),
        Some(&original.gallery.presets[4])
    );
    let before = profiles.clone();
    persist_candidate_state(&mut state, &path).unwrap();
    for _ in 0..3 {
        state = CandidateState::load(&path);
        assert_eq!(state.gallery.presets, original.gallery.presets);
        assert_eq!(state.mode_profiles, before);
    }
}

#[test]
fn p4_frozen_v2_raw_preflight_refuses_v3_primary_and_backup_before_typed_decode() {
    // Frozen v2 raw-version policy from the retained pre-P4 candidate_state.rs.
    // This fixture does not claim execution of an older full application.
    const FROZEN_V2: i64 = 2;
    fn preflight(path: &Path) -> Result<(), i64> {
        for file in [path.to_path_buf(), path.with_extension("toml.bak")] {
            let text = fs::read_to_string(file).unwrap();
            let raw: toml::Value = toml::from_str(&text).unwrap();
            let version = raw
                .get("version")
                .and_then(toml::Value::as_integer)
                .unwrap_or(0);
            if version > FROZEN_V2 {
                return Err(version);
            }
        }
        Ok(())
    }
    for backup_future in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "p4-frozen-v2-{}-{backup_future}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        let backup = path.with_extension("toml.bak");
        let current = b"version=2\nfavorites=['old']";
        let future = b"# incompatible typed field is deliberately never decoded\nversion=3\ngallery='new format'";
        fs::write(
            &path,
            if backup_future {
                current.as_slice()
            } else {
                future.as_slice()
            },
        )
        .unwrap();
        fs::write(
            &backup,
            if backup_future {
                future.as_slice()
            } else {
                current.as_slice()
            },
        )
        .unwrap();
        let before = (fs::read(&path).unwrap(), fs::read(&backup).unwrap());
        assert_eq!(preflight(&path), Err(3));
        assert_eq!(
            (fs::read(&path).unwrap(), fs::read(&backup).unwrap()),
            before
        );
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    }
}

#[test]
fn p4_both_publication_owners_block_retry_after_observed_primary_or_backup_failure() {
    fn bytes(root: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
        let mut result: Vec<_> = fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
    for use_proposal in [false, true] {
        for (point, stage) in [
            (SaveFailurePoint::PublishPrimary, SaveStage::PublishPrimary),
            (SaveFailurePoint::PublishBackup, SaveStage::PublishBackup),
        ] {
            for (index, effect) in [
                PublicationEffect::DestinationUnchanged,
                PublicationEffect::DestinationAbsent,
                PublicationEffect::OriginalMovedToKnownRecovery,
            ]
            .into_iter()
            .enumerate()
            {
                let root = std::env::temp_dir().join(format!(
                    "p4-owner-reconcile-{}-{use_proposal}-{point:?}-{index}",
                    std::process::id()
                ));
                fs::create_dir_all(&root).unwrap();
                let path = root.join("state.toml");
                let mut live = CandidateState::default();
                live.favorites.insert("first".into());
                persist_candidate_state(&mut live, &path).unwrap();
                live.favorites.insert("second".into());
                persist_candidate_state(&mut live, &path).unwrap();
                let saved = live.favorites.clone();
                let error = if use_proposal {
                    let mut proposed = live.clone();
                    proposed.favorites.insert("proposed".into());
                    let error = commit_saved_state_using(&mut live, proposed, &path, |state| {
                        state.save_with_publication_effect(&path, point, effect)
                    })
                    .unwrap_err();
                    assert_eq!(live.favorites, saved, "failed proposal must not activate");
                    error
                } else {
                    live.favorites.insert("edited-memory".into());
                    let error = persist_candidate_state_using(&mut live, |state| {
                        state.save_with_publication_effect(&path, point, effect)
                    })
                    .unwrap_err();
                    assert!(
                        live.favorites.contains("edited-memory"),
                        "ordinary edits remain usable"
                    );
                    error
                };
                assert_eq!(error.stage, stage);
                assert!(error.primary.is_some());
                assert_eq!(live.reconciliation_required.as_ref(), Some(&error));
                let observed = bytes(&root);
                assert!(persist_candidate_state(&mut live, &path)
                    .unwrap_err()
                    .to_string()
                    .contains("deliberately reload"));
                assert!(commit_saved_state(&mut live, CandidateState::default(), &path).is_err());
                assert_eq!(
                    bytes(&root),
                    observed,
                    "retry must not touch disk or recovery evidence"
                );
                assert_eq!(
                    live.reconciliation_required.as_ref(),
                    Some(&error),
                    "refused retries retain the original publication receipt"
                );
                let mut reloaded = CandidateState::load(&path);
                assert!(reloaded.reconciliation_required.is_none());
                persist_candidate_state(&mut reloaded, &path).unwrap();
                for recovery in error.recovery_paths() {
                    assert!(recovery.exists());
                }
            }
        }
    }
}

#[test]
fn p4_typed_fractional_profiles_and_pending_json_roundtrip_without_active_promotion() {
    use frontend::{DeferredCapability, PendingOnly, PendingSetting};
    let mut state = CandidateState::default();
    let raw="version=2\n[[gallery.presets]]\nname='Depth'\nmode='depth'\n[gallery.presets.depth]\nvisible_count=7\nnavigation_ms=321\n";
    let legacy = parse_state_contents(raw).unwrap();
    let mut snapshot = legacy.mode_profiles.depth.entries[0]
        .snapshot
        .clone()
        .unwrap();
    snapshot.visible_count = 7.25;
    snapshot.navigation_ms = 321.5;
    state
        .mode_profiles
        .save(mode_profiles::Snapshot::Depth(snapshot.clone()), |n| {
            format!("Layout {n}")
        })
        .unwrap();
    for (index, value) in [
        "null",
        "true",
        "1.25",
        r#"[true,"text",null,3]"#,
        r#"{"nested":[null,{"enabled":true}]}"#,
    ]
    .into_iter()
    .enumerate()
    {
        state.pending_frontend.settings.push(PendingSetting {
            capability: DeferredCapability::ExpandedScheduling,
            source_path: format!("preserved-unknown-{index}"),
            value_json: value.into(),
            status: PendingOnly::Pending,
        });
    }
    let encoded = toml::to_string_pretty(&state).unwrap();
    let restarted = parse_state_contents(&encoded).unwrap();
    assert_eq!(restarted.mode_profiles, state.mode_profiles);
    assert_eq!(restarted.pending_frontend, state.pending_frontend);
    assert!(!restarted.slideshow_enabled);
    assert!(restarted.schedules.is_empty());
    let mut duplicate = state.clone();
    duplicate
        .mode_profiles
        .depth
        .entries
        .push(duplicate.mode_profiles.depth.entries[0].clone());
    assert!(duplicate.validate_frontend().is_err());
    let mut invalid = state;
    invalid.card_flip.as_mut().unwrap().duration_ms = 0;
    assert!(invalid.validate_frontend().is_err());
}

#[test]
fn p4_pending_scalar_source_normalization_is_inert_and_rejects_invalid_edits() {
    use frontend::DeferredCapability as C;
    use serde_json::json;
    let cases = [
        (
            C::ThemeGeneration,
            "matugen.colorIndex",
            json!("2"),
            json!(2),
        ),
        (
            C::ThemeGeneration,
            "matugen.contrast",
            json!("-0.125"),
            json!(-0.125),
        ),
        (
            C::ExpandedScheduling,
            "schedule.latitude",
            json!(12.25),
            json!("12.25"),
        ),
        (
            C::ExpandedScheduling,
            "schedule.longitude",
            json!(""),
            json!(""),
        ),
        (
            C::MediaPlaybackPolicy,
            "videoPreview.delayMs",
            json!(-5.25),
            json!(-5.25),
        ),
        (
            C::SourceBrowsing,
            "sources.youtube.maxMinutes",
            json!(700.5),
            json!(700.5),
        ),
        (
            C::Optimization,
            "performance.batteryFps",
            json!(0),
            json!(0),
        ),
        (
            C::PlatformIntegration,
            "library.pollingIntervalSeconds",
            json!(1),
            json!(15),
        ),
        (
            C::SemanticSearch,
            "tagging.defaultSearchMode",
            json!("describe"),
            json!("describe"),
        ),
    ];
    for (capability, path, raw, expected) in cases {
        assert_eq!(
            pending::normalize(capability, path, raw).unwrap(),
            expected,
            "{path}"
        );
    }
    for (capability, path, raw) in [
        (C::ThemeGeneration, "matugen.contrast", json!("")),
        (C::ThemeGeneration, "matugen.contrast", json!("nan")),
        (C::ThemeGeneration, "matugen.colorIndex", json!(1.25)),
        (C::ExpandedScheduling, "schedule.applyOnStart", json!(null)),
        (
            C::SourceBrowsing,
            "wallhaven.defaults.sort",
            json!("invented"),
        ),
        (C::Optimization, "performance.gpuPreference", json!(false)),
        (C::PlatformIntegration, "paper.videoEngine", json!("tinier")),
        (C::SourceBrowsing, "theme.mode", json!("dark")),
        (C::ThemeGeneration, "theme.unlisted", json!(true)),
    ] {
        assert!(pending::normalize(capability, path, raw).is_err(), "{path}");
    }
}

#[test]
fn p4_unknown_pending_keys_and_future_choices_survive_save_reload_without_activation() {
    use frontend::{DeferredCapability as C, PendingOnly, PendingSetting};
    let root = std::env::temp_dir().join(format!("p4-pending-preservation-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let mut state = CandidateState::default();
    state.pending_frontend.settings = vec![
        PendingSetting {
            capability: C::ThemeGeneration,
            source_path: "theme.futureChoice".into(),
            value_json: r#"{"nullable":null,"ordered":[1,true,"x"]}"#.into(),
            status: PendingOnly::Pending,
        },
        PendingSetting {
            capability: C::ThemeGeneration,
            source_path: "theme.mode".into(),
            value_json: r#""future-variant""#.into(),
            status: PendingOnly::Pending,
        },
        PendingSetting {
            capability: C::ExpandedScheduling,
            source_path: "schedule.applyOnStart".into(),
            value_json: "true".into(),
            status: PendingOnly::Pending,
        },
    ];
    let pending = state.pending_frontend.clone();
    persist_candidate_state(&mut state, &path).unwrap();
    let raw = fs::read(&path).unwrap();
    let loaded = CandidateState::load_with_diagnostics(&path);
    assert_eq!(fs::read(&path).unwrap(), raw);
    assert_eq!(loaded.state.pending_frontend, pending);
    assert!(loaded.diagnostic.unwrap().contains("retained inert"));
    assert!(!loaded.state.slideshow_enabled);
    assert!(loaded.state.schedules.is_empty());
    let mut restarted = loaded.state;
    persist_candidate_state(&mut restarted, &path).unwrap();
    assert_eq!(CandidateState::load(&path).pending_frontend, pending);
}
