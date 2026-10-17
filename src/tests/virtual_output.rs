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

#[test]
fn virtual_output_ipc_roundtrip_and_naming() {
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
    assert_eq!(name, "sunshine");

    // The IPC entry carries the real name; make + model give the portal its display name
    // ("niri sunshine"), not a generic "niri virtual".
    {
        let state = f.niri_state();
        let ipc_map = state.backend.ipc_outputs();
        let ipc = ipc_map.lock().unwrap();
        let ipc_output = ipc.values().find(|o| o.name == "sunshine").unwrap();
        assert_eq!(ipc_output.make, "niri");
        assert_eq!(ipc_output.model, "sunshine");
        assert_eq!(ipc_output.modes.len(), 1);
        assert!(ipc_output.current_mode.is_some());
        assert!(ipc_output.logical.is_some());
    }

    {
        let state = f.niri_state();
        state
            .backend
            .remove_virtual_output(&mut state.niri, "sunshine")
            .unwrap();
    }

    assert!(!f
        .niri()
        .global_space
        .outputs()
        .any(|o| o.name() == "sunshine"));
    {
        let state = f.niri_state();
        let ipc_map = state.backend.ipc_outputs();
        let ipc = ipc_map.lock().unwrap();
        assert!(!ipc.values().any(|o| o.name == "sunshine"));
    }
}

#[test]
fn virtual_output_off_on_cycle() {
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

    let output = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();

    // off: disconnected from clients, IPC entry kept with logical: None.
    {
        let state = f.niri_state();
        state.apply_transient_output_config(&name, niri_ipc::OutputAction::Off);
    }
    assert!(!f.niri().global_space.outputs().any(|o| o.name() == name));
    assert!(!f.niri().output_state.contains_key(&output));
    {
        let state = f.niri_state();
        let ipc_map = state.backend.ipc_outputs();
        let ipc = ipc_map.lock().unwrap();
        let ipc_output = ipc.values().find(|o| o.name == name).unwrap();
        assert!(ipc_output.logical.is_none());
    }

    // on: reconnected with a working frame clock.
    {
        let state = f.niri_state();
        state.apply_transient_output_config(&name, niri_ipc::OutputAction::On);
    }
    assert!(f.niri().global_space.outputs().any(|o| o.name() == name));
    let state = f.niri().output_state.get(&output).unwrap();
    assert!(state.frame_clock.refresh_interval().is_some());
    {
        let state = f.niri_state();
        let ipc_map = state.backend.ipc_outputs();
        let ipc = ipc_map.lock().unwrap();
        let ipc_output = ipc.values().find(|o| o.name == name).unwrap();
        assert!(ipc_output.logical.is_some());
    }
}

#[test]
fn virtual_output_mode_change_while_off() {
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

    let output = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();

    {
        let state = f.niri_state();
        state.apply_transient_output_config(&name, niri_ipc::OutputAction::Off);
    }

    // The mode is ready when the output is re-enabled.
    {
        let state = f.niri_state();
        state.apply_transient_output_config(
            &name,
            niri_ipc::OutputAction::CustomMode {
                mode: niri_ipc::ConfiguredMode {
                    width: 1280,
                    height: 720,
                    refresh: Some(30.0),
                },
            },
        );
    }
    let mode = output.current_mode().unwrap();
    assert_eq!(mode.size.w, 1280);
    assert_eq!(mode.size.h, 720);
    assert_eq!(mode.refresh, 30_000);

    {
        let state = f.niri_state();
        state.apply_transient_output_config(&name, niri_ipc::OutputAction::On);
    }
    let mode = output.current_mode().unwrap();
    assert_eq!(mode.size.w, 1280);
    let state = f.niri().output_state.get(&output).unwrap();
    let interval = state.frame_clock.refresh_interval().unwrap();
    assert!(
        (interval.as_nanos() as f64 - 1e9 / 30.0).abs() < 1e6,
        "unexpected frame clock interval: {interval:?}",
    );
}

#[test]
fn virtual_output_pacing_advances_frame_sequence() {
    use std::time::{Duration, Instant};

    use calloop::timer::{TimeoutAction, Timer};

    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(niri_ipc::ConfiguredMode {
                    width: 800,
                    height: 600,
                    refresh: Some(100.0),
                }),
                Some("virt".to_owned()),
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

    // Keep the output busy so the synthetic clock keeps producing frames: a fast timer queues
    // a redraw and pumps the compositor (including the server event loop where the output's
    // estimated-vblank timer lives). The vblank timer then paces actual renders at the mode
    // rate (a queued redraw mid-wait must not jump the queue).
    let output_for_timer = output.clone();
    f.handle
        .insert_source(
            Timer::from_duration(Duration::from_millis(2)),
            move |_, _, state| {
                state.server.state.niri.queue_redraw(&output_for_timer);
                state.server.dispatch();
                TimeoutAction::ToDuration(Duration::from_millis(2))
            },
        )
        .unwrap();

    f.dispatch();

    let start = f.niri().output_state[&output].frame_callback_sequence;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(300) {
        std::thread::sleep(Duration::from_millis(5));
        f.dispatch();
    }
    let elapsed = t0.elapsed();
    let end = f.niri().output_state[&output].frame_callback_sequence;
    let delta = end.wrapping_sub(start);

    // The sequence advances twice per paced frame (once on render, once on the synthetic
    // vblank). Loose bounds: this must be far above zero (clock dead) and far below the
    // unthrottled driver rate (500 Hz queueing).
    let frames = elapsed.as_secs_f64() * 100.0;
    assert!(
        (delta as f64) >= frames && (delta as f64) <= frames * 4.0,
        "frame_callback_sequence advanced by {delta} in {elapsed:?}; expected ~{} (paced at 100 Hz)",
        frames as u32,
    );
}

#[test]
fn virtual_output_scale_affects_logical_geometry_and_mapping() {
    let mut f = Fixture::new();

    // A scale-1 physical-ish output at the origin.
    f.add_output(1, (1280, 720));

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(niri_ipc::ConfiguredMode {
                    width: 1920,
                    height: 1080,
                    refresh: Some(60.0),
                }),
                Some("virt".to_owned()),
            )
            .unwrap()
    };

    {
        let state = f.niri_state();
        state.apply_transient_output_config(
            &name,
            niri_ipc::OutputAction::Scale {
                scale: niri_ipc::ScaleToSet::Specific(2.0),
            },
        );
    }

    let virt = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();

    // Logical size is physical size / scale, so absolute input mapping uses the scaled size.
    let geom = f.niri().global_space.output_geometry(&virt).unwrap();
    assert_eq!(geom.size.w, 960);
    assert_eq!(geom.size.h, 540);

    // The virtual output is placed to the right of the first one, and absolute input mapping
    // uses logical (scaled) geometry: a point in the virtual output's logical rect belongs to
    // it, not to the first output.
    assert_eq!(geom.loc.x, 1280);
    let first = f.niri().global_space.outputs().next().unwrap().clone();
    let first_geom = f.niri().global_space.output_geometry(&first).unwrap();
    assert_eq!(first_geom.size.w, 1280);
    let inside = |g: smithay::utils::Rectangle<i32, smithay::utils::Logical>, p: (i32, i32)| {
        p.0 >= g.loc.x && p.0 < g.loc.x + g.size.w && p.1 >= g.loc.y && p.1 < g.loc.y + g.size.h
    };
    let point = (1300, 100);
    assert!(!inside(first_geom, point));
    assert!(inside(geom, point));
}

/// Insert a fast timer that pumps the server event loop, so the virtual output's synthetic
/// vblank timers fire while the test's pump loop runs.
fn pump_server_every_ms(f: &mut Fixture, ms: u64) {
    use calloop::timer::{TimeoutAction, Timer};

    let interval = std::time::Duration::from_millis(ms);
    f.handle
        .insert_source(Timer::from_duration(interval), move |_, _, state| {
            state.server.dispatch();
            TimeoutAction::ToDuration(interval)
        })
        .unwrap();
}

/// Pump the fixture (server) and client loops for `duration`, making the window (if given)
/// commit a new frame with a fresh frame callback + presentation feedback on every frame
/// callback, like a screen-capture client does.
fn pump_presentation_loops(
    f: &mut Fixture,
    id: client::ClientId,
    window_surface: Option<wayland_client::protocol::wl_surface::WlSurface>,
    duration: std::time::Duration,
) -> std::time::Duration {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < duration {
        f.dispatch();
        let client = f.client(id);
        client.dispatch();
        if let Some(surface) = &window_surface {
            client.window(surface).pump_presentation_loop();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    t0.elapsed()
}

/// Pump until `cond` holds or `timeout` elapses. Returns whether the condition was met.
fn pump_until(
    f: &mut Fixture,
    id: client::ClientId,
    timeout: std::time::Duration,
    cond: impl Fn(&client::Client) -> bool,
) -> bool {
    let t0 = std::time::Instant::now();
    loop {
        f.dispatch();
        f.client(id).dispatch();
        if cond(f.client(id)) {
            return true;
        }
        if t0.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// A fixture with a single virtual output at `mode` and one client window on it, running a
/// commit-on-every-frame-callback loop with presentation feedbacks.
fn presentation_fixture(
    mode: niri_ipc::ConfiguredMode,
) -> (
    Fixture,
    client::ClientId,
    wayland_client::protocol::wl_surface::WlSurface,
) {
    let mut f = Fixture::new();

    let _name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(&mut state.niri, Some(mode), Some("virt".to_owned()))
            .unwrap()
    };

    pump_server_every_ms(&mut f, 1);

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);

    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    let window = f.client(id).window(&surface);
    window.start_presentation_loop();

    (f, id, surface)
}

fn presented_count(
    f: &Fixture,
    id: client::ClientId,
    surface: &wayland_client::protocol::wl_surface::WlSurface,
) -> usize {
    f.state
        .clients
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .state
        .windows
        .iter()
        .find(|w| w.surface == *surface)
        .unwrap()
        .presented
}

/// The presentation-feedback path must deliver `wp_presentation_feedback.presented` events at
/// the output's mode rate: a client that commits on every frame callback and requests feedback
/// each frame (what a screen-capture client does) gets one presented event per rendered frame.
#[test]
fn virtual_output_presentation_feedback_paces_at_mode_rate() {
    let (mut f, id, surface) = presentation_fixture(niri_ipc::ConfiguredMode {
        width: 800,
        height: 600,
        refresh: Some(100.0),
    });

    let elapsed = pump_presentation_loops(
        &mut f,
        id,
        Some(surface.clone()),
        std::time::Duration::from_millis(1000),
    );
    let presented = presented_count(&f, id, &surface);

    let expected = elapsed.as_secs_f64() * 100.0;
    // Loose bounds for CI: the feedback rate tracks the mode rate, never exceeds it, and is
    // nowhere near zero (which is what a broken feedback path looks like: the client stalls
    // waiting for frame callbacks and presents grind to a halt).
    assert!(
        presented as f64 >= expected * 0.6 && presented as f64 <= expected * 1.3,
        "{presented} presented events in {elapsed:?}; expected ~{expected:.0} at 100 Hz",
    );
}

/// The 4 s @ 2560x1440@120 measurement for the report: prints presented count and the
/// presented-to-presented interval. Run with:
/// `cargo test --release virtual_output_presentation_feedback_2560x1440_at_120hz -- --ignored
/// --nocapture`
#[test]
#[ignore = "4 s measurement at 2560x1440@120; run with --ignored --nocapture"]
fn virtual_output_presentation_feedback_2560x1440_at_120hz() {
    let (mut f, id, surface) = presentation_fixture(niri_ipc::ConfiguredMode {
        width: 2560,
        height: 1440,
        refresh: Some(120.0),
    });

    let elapsed = pump_presentation_loops(
        &mut f,
        id,
        Some(surface.clone()),
        std::time::Duration::from_secs(4),
    );

    let window = f
        .state
        .client(id)
        .state
        .windows
        .iter()
        .find(|w| w.surface == surface)
        .unwrap();
    let presented = window.presented;
    let discarded = window.feedback_discarded;
    let stamps = &window.presented_at;

    println!("elapsed: {elapsed:?}");
    println!("presented: {presented}, discarded: {discarded}");
    println!(
        "rate: {:.1} Hz (expected ~480 in 4 s at 120 Hz)",
        presented as f64 / elapsed.as_secs_f64()
    );
    if stamps.len() >= 2 {
        let mut diffs: Vec<f64> = stamps
            .windows(2)
            .map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0)
            .collect();
        diffs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean: f64 = diffs.iter().sum::<f64>() / diffs.len() as f64;
        println!(
            "p2p interval: mean {mean:.2} ms, median {:.2} ms, p10 {:.2} ms, p90 {:.2} ms (expected ~8.33 ms)",
            diffs[diffs.len() / 2],
            diffs[diffs.len() / 10],
            diffs[diffs.len() * 9 / 10],
        );
    }

    assert!(presented > 0, "no presented events");
}

/// Virtual outputs keep rendering (and pacing) while physical monitors are powered off via
/// `power-off-monitors` (DPMS): their buffers feed capture clients, unlike a CRTC with no
/// framebuffer attached.
#[test]
fn virtual_output_keeps_presenting_while_monitors_powered_off() {
    let (mut f, id, surface) = presentation_fixture(niri_ipc::ConfiguredMode {
        width: 800,
        height: 600,
        refresh: Some(100.0),
    });

    // Baseline with monitors on.
    pump_presentation_loops(
        &mut f,
        id,
        Some(surface.clone()),
        std::time::Duration::from_millis(500),
    );
    let baseline = presented_count(&f, id, &surface);
    assert!(baseline > 0, "no presented events with monitors on");

    {
        let state = f.niri_state();
        state.niri.deactivate_monitors(&mut state.backend);
    }

    let before = presented_count(&f, id, &surface);
    let elapsed = pump_presentation_loops(
        &mut f,
        id,
        Some(surface.clone()),
        std::time::Duration::from_millis(500),
    );
    let presented = presented_count(&f, id, &surface) - before;

    let expected = elapsed.as_secs_f64() * 100.0;
    assert!(
        presented as f64 >= expected * 0.6,
        "{presented} presented events in {elapsed:?} with monitors powered off; \
         the virtual output must keep rendering at its mode rate (expected ~{expected:.0})",
    );
}

/// Locking while monitors are powered off must actually render the lock surface on the virtual
/// output before the session counts as locked: the output keeps displaying its buffer to
/// capture clients, so it can't be treated like a powered-off CRTC with nothing attached.
#[test]
fn lock_while_monitors_powered_off_renders_lock_surface() {
    use std::sync::atomic::Ordering;

    let mut f = Fixture::new();

    let name = {
        let state = f.niri_state();
        state
            .backend
            .create_virtual_output(
                &mut state.niri,
                Some(niri_ipc::ConfiguredMode {
                    width: 800,
                    height: 600,
                    refresh: Some(100.0),
                }),
                Some("virt".to_owned()),
            )
            .unwrap()
    };

    pump_server_every_ms(&mut f, 1);

    let id = f.add_client();
    // Let the output name events arrive before looking the output up by name.
    f.double_roundtrip(id);

    {
        let state = f.niri_state();
        state.niri.deactivate_monitors(&mut state.backend);
    }

    let client_output = f.client(id).output(&name);
    {
        let client = f.client(id);
        client.lock(&client_output);
    }

    // Wait for the lock surface configure.
    let configured = pump_until(&mut f, id, std::time::Duration::from_secs(2), |c| {
        c.state.locks.first().is_some_and(|l| l.configure.is_some())
    });
    assert!(configured, "no configure for the lock surface");

    // Commit the lock surface, asking for a presentation feedback with the frame.
    f.client(id).state.locks[0].ack_and_commit_with_feedback();

    // The session must become locked, and only after the lock surface was actually rendered
    // (the feedback is presented from the render path). Before the fix, `locked` arrived via
    // the powered-off-CRTC shortcut with zero presented feedbacks.
    let locked = pump_until(&mut f, id, std::time::Duration::from_secs(2), |c| {
        c.state
            .locks
            .first()
            .is_some_and(|l| l.locked.load(Ordering::Relaxed))
    });
    assert!(locked, "session never locked");

    let lock = &f.client(id).state.locks[0];
    assert!(
        lock.presented >= 1,
        "session locked without rendering the lock surface (presented: {}, discarded: {})",
        lock.presented,
        lock.feedback_discarded,
    );
    assert!(
        !lock.finished.load(Ordering::Relaxed),
        "session lock was finished (unlocked) unexpectedly",
    );
}

#[test]
fn tablet_map_to_output_targets_virtual_output() {
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

    {
        let state = f.niri_state();
        let mut config = state.niri.config.borrow_mut();
        config.input.tablet.map_to_focused_output = false;
        config.input.tablet.map_to_output = Some(name.clone());
    }

    // Focus the other output; the tablet must still map to the virtual one.
    f.niri_focus_output(1);

    let virt = f
        .niri()
        .global_space
        .outputs()
        .find(|o| o.name() == name)
        .unwrap()
        .clone();
    assert_eq!(f.niri().output_for_tablet(), Some(&virt));
}
