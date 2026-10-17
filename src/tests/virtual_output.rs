use super::*;

/// The mode the old positional `create_virtual_output` API defaulted to.
fn default_mode() -> niri_ipc::ConfiguredMode {
    niri_ipc::ConfiguredMode {
        width: 1920,
        height: 1080,
        refresh: Some(60.0),
    }
}

#[test]
fn virtual_output_custom_mode_does_not_accumulate_modes() {
    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(default_mode()),
                Some("sunshine".to_owned()),
            )
            .unwrap()
    };

    let output = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();

    // Sanity: single initial mode.
    {
        let modes = output.modes();
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].size.w, 1920);
        assert_eq!(modes[0].size.h, 1080);
    }

    // 1080p -> 3200x1800
    {
        let state = f.niri_state();
        state.apply_transient_output_config(
            &name,
            niri_ipc::OutputAction::CustomMode {
                mode: niri_ipc::ConfiguredMode {
                    width: 3200,
                    height: 1800,
                    refresh: Some(60.0),
                },
            },
        );
    }

    {
        let modes = output.modes();
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].size.w, 3200);
        assert_eq!(modes[0].size.h, 1800);
    }

    // 3200x1800 -> 1080p
    {
        let state = f.niri_state();
        state.apply_transient_output_config(
            &name,
            niri_ipc::OutputAction::CustomMode {
                mode: niri_ipc::ConfiguredMode {
                    width: 1920,
                    height: 1080,
                    refresh: Some(60.0),
                },
            },
        );
    }

    {
        let modes = output.modes();
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].size.w, 1920);
        assert_eq!(modes[0].size.h, 1080);
    }
}

#[test]
fn removing_off_virtual_output_does_not_panic() {
    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(default_mode()),
                Some("virt".to_owned()),
            )
            .unwrap()
    };

    {
        let state = f.niri_state();
        state.modify_output_config(&name, |config| {
            config.off = true;
        });
    }

    {
        let state = f.niri_state();
        state
            .backend
            .remove_virtual_output(&mut state.niri, &name)
            .unwrap();
    }
}

#[test]
fn config_remove_virtual_output_fails_while_configured() {
    let mut f = Fixture::new();
    let name = "virt";

    {
        let state = f.niri_state();
        state.modify_output_config(name, |config| {
            config.create_virtual = true;
        });
    }

    let result = {
        let state = f.niri_state();
        state.backend.remove_virtual_output(&mut state.niri, name)
    };

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("is configured"));
}

#[test]
fn unnamed_virtual_outputs_do_not_advance_counter_for_named_outputs() {
    let mut f = Fixture::new();

    {
        let state = f.niri_state();
        state.modify_output_config("virt", |_| {});
    }

    let headless_1 = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(&mut state.niri, Some(default_mode()), None)
            .unwrap()
    };

    assert_eq!(headless_1, "HEADLESS-1");

    let headless_2 = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(&mut state.niri, Some(default_mode()), None)
            .unwrap()
    };

    assert_eq!(headless_2, "HEADLESS-2");
}

#[test]
fn touch_input_targets_virtual_output_when_focused() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(default_mode()),
                Some("virt".to_owned()),
            )
            .unwrap()
    };

    let virt = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();

    f.niri().layout.focus_output(&virt);

    // With no explicit `input.touch.map-to-output` configured,
    // touch should follow the active output (which may be virtual).
    let touch_output = f.niri().output_for_touch().unwrap().clone();
    assert_eq!(touch_output, virt);
}

#[test]
fn create_virtual_output_resizes_in_place() {
    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(default_mode()),
                Some("sunshine".to_owned()),
            )
            .unwrap()
    };

    let output = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();
    let output_count = f.niri().global_space.outputs().count();

    // Re-creating with the same name resizes in place instead of erroring.
    {
        let state = f.niri_state();
        let created = state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(niri_ipc::ConfiguredMode {
                    width: 1280,
                    height: 720,
                    refresh: Some(120.0),
                }),
                Some("sunshine".to_owned()),
            )
            .unwrap();
        assert_eq!(created, name);
    }

    // The same output object is reused; no new output was created.
    assert_eq!(f.niri().global_space.outputs().count(), output_count);
    let modes = output.modes();
    assert_eq!(modes.len(), 1);
    assert_eq!(modes[0].size.w, 1280);
    assert_eq!(modes[0].size.h, 720);
    assert_eq!(modes[0].refresh, 120_000);

    // The frame clock was rebuilt at the new refresh rate.
    let state = f
        .niri()
        .output_state
        .get(&output)
        .expect("output is connected");
    let interval = state.frame_clock.refresh_interval().unwrap();
    assert!(
        (interval.as_nanos() as f64 - 1e9 / 120.0).abs() < 1e6,
        "unexpected frame clock interval: {interval:?}",
    );

    // Re-creating without a mode is a no-op.
    {
        let state = f.niri_state();
        let created = state
            .backend
            .create_virtual_output(&mut state.niri, None, Some("sunshine".to_owned()))
            .unwrap();
        assert_eq!(created, name);
    }
    assert_eq!(output.modes()[0].size.w, 1280);
}

#[test]
fn create_virtual_output_validates_mode() {
    let mut f = Fixture::new();

    let cases = [
        niri_ipc::ConfiguredMode {
            width: 8,
            height: 1080,
            refresh: Some(60.0),
        },
        niri_ipc::ConfiguredMode {
            width: 1920,
            height: 0,
            refresh: Some(60.0),
        },
        niri_ipc::ConfiguredMode {
            width: 1920,
            height: 1080,
            refresh: Some(0.0),
        },
        niri_ipc::ConfiguredMode {
            width: 1920,
            height: 1080,
            refresh: Some(1001.0),
        },
    ];

    for mode in cases {
        let state = f.niri_state();
        let result = state
            .backend
            .create_virtual_output(&mut state.niri, Some(mode), None);
        assert!(result.is_err(), "expected {mode:?} to be rejected");
    }
}

#[test]
fn virtual_output_scale_applies_via_transient_config() {
    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(&mut state.niri, Some(default_mode()), None)
            .unwrap()
    };

    let output = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();
    assert_eq!(output.current_scale().fractional_scale(), 1.0);

    {
        let state = f.niri_state();
        state.apply_transient_output_config(
            &name,
            niri_ipc::OutputAction::Scale {
                scale: niri_ipc::ScaleToSet::Specific(2.0),
            },
        );
    }

    assert_eq!(output.current_scale().fractional_scale(), 2.0);
}
