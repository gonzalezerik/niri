//! Reproduction/regression tests for the color-management protocol handlers. These drive the same
//! requests that real clients (wayland-info, mpv gpu-next) send. Because niri builds smithay with
//! `use_system_lib`, a panic in a server dispatch handler unwinds across the C libwayland frame and
//! aborts the process — so any handler panic shows up here as a failing test.

use niri_config::output::Hdr;
use niri_config::{Config, Output};
use smithay::reexports::wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::{
    Primaries, RenderIntent, TransferFunction,
};

use wayland_client::protocol::wl_surface::WlSurface;

use super::client::ClientId;
use super::*;
use crate::backend::OutputHdrCaps;

/// A fixture whose config opts an output into HDR, so the (gated) `wp_color_manager_v1` global is
/// advertised. Without an HDR-enabled output, niri does not advertise color management at all.
fn fixture_with_hdr() -> Fixture {
    let mut config = Config::default();
    config.outputs.0.push(Output {
        name: "headless-1".to_owned(),
        hdr: Some(Hdr::default()),
        ..Default::default()
    });
    Fixture::with_config(config)
}

#[test]
fn global_is_advertised_and_bound() {
    let mut f = fixture_with_hdr();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    f.double_roundtrip(id);

    assert!(
        f.client(id).state.color_manager.is_some(),
        "wp_color_manager_v1 was not advertised/bound"
    );
}

#[test]
fn global_not_advertised_without_hdr_config() {
    // Default config (no `hdr` on any output) must not advertise color management.
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    f.double_roundtrip(id);

    assert!(
        f.client(id).state.color_manager.is_none(),
        "wp_color_manager_v1 must not be advertised without an HDR-enabled output"
    );
}

#[test]
fn probe_output_image_description_like_wayland_info() {
    let mut f = fixture_with_hdr();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    f.double_roundtrip(id);

    // get_output -> get_image_description -> get_information, as wayland-info does.
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
}

#[test]
fn create_parametric_hdr_description_like_mpv() {
    let mut f = fixture_with_hdr();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);

    // create_parametric_creator -> set BT.2020 + PQ + mastering metadata -> create -> attach to the
    // surface, as mpv --vo=gpu-next does for HDR content.
    f.client(id).create_and_attach_hdr_description(
        &surface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    f.double_roundtrip(id);
}

/// Fixture whose output is `hdr mode="on"` with backend HDR capabilities injected, simulating an
/// HDR-capable monitor on the TTY backend.
fn fixture_with_hdr_mode_on() -> Fixture {
    use niri_config::output::HdrMode;

    let mut config = Config::default();
    config.outputs.0.push(Output {
        name: "headless-1".to_owned(),
        hdr: Some(Hdr {
            mode: Some(HdrMode::On),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1920, 1080));

    // The headless backend doesn't probe HDR capabilities; inject them like the TTY backend would.
    f.niri_output(1)
        .user_data()
        .insert_if_missing(|| OutputHdrCaps {
            supported: true,
            max_luminance: 800,
            min_luminance: 100,
            max_frame_avg_luminance: 600,
        });
    f
}

#[test]
fn feedback_preferred_defaults_to_srgb() {
    let mut f = fixture_with_hdr();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);

    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);

    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::Srgb));
    assert_eq!(client.state.info_primaries, Some(Primaries::Srgb));
}

#[test]
fn feedback_preferred_is_pq_with_mode_on() {
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    // An SDL3-style client probes the preferred description once at startup, before going
    // fullscreen. With mode "on" it must see PQ/BT.2020 right away.
    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);

    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::St2084Pq));
    assert_eq!(client.state.info_primaries, Some(Primaries::Bt2020));
}

/// Fixture whose output is `hdr mode="auto"` with backend HDR capabilities injected.
fn fixture_with_hdr_mode_auto() -> Fixture {
    use niri_config::output::HdrMode;

    let mut config = Config::default();
    config.outputs.0.push(Output {
        name: "headless-1".to_owned(),
        hdr: Some(Hdr {
            mode: Some(HdrMode::Auto),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1920, 1080));
    f.niri_output(1)
        .user_data()
        .insert_if_missing(|| OutputHdrCaps {
            supported: true,
            max_luminance: 800,
            min_luminance: 100,
            max_frame_avg_luminance: 600,
        });
    f
}

#[test]
fn feedback_preferred_is_pq_before_fullscreen_ack() {
    // gamescope (nested) unmaps its toplevel while the game inside switches windows, then maps
    // it again and requests fullscreen. It re-reads the preferred description right away, before
    // acking the fullscreen configure, and the game decides on HDR at that moment. In auto mode a
    // window that has been told to go fullscreen must already prefer PQ.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    // Request fullscreen, but don't ack the resulting configure yet.
    f.client(id).window(&surface).set_fullscreen(None);
    f.double_roundtrip(id);

    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);

    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::St2084Pq));
    assert_eq!(client.state.info_primaries, Some(Primaries::Bt2020));
}

#[test]
fn inactive_fullscreen_window_prefers_pq_and_connector_stays_sdr() {
    // A fullscreen window that is not the active window must still be told to prefer PQ: the
    // gamescope WSI layer reads HDR support once at vkCreateSurface and never again, so a game
    // that starts without focus would otherwise never see HDR. The connector must stay SDR
    // though — engagement still requires the *active* fullscreen window to carry HDR content.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    // A second window opens. niri deliberately doesn't steal focus from an active fullscreen
    // window, so move focus explicitly to make the fullscreen window inactive.
    let id2 = f.add_client();
    let other = f.client(id2).create_window();
    let other_surface = other.surface.clone();
    other.commit();
    f.roundtrip(id2);
    let other = f.client(id2).window(&other_surface);
    other.attach_new_buffer();
    other.ack_last_and_commit();
    f.double_roundtrip(id2);
    f.niri().layout.focus_right();
    f.double_roundtrip(id2);

    // The inactive fullscreen window carries an HDR description...
    f.client(id).create_and_attach_hdr_description(
        &surface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    surface.commit();
    f.double_roundtrip(id);

    // ...and is told to prefer PQ even though it is not active...
    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::St2084Pq));
    assert_eq!(client.state.info_primaries, Some(Primaries::Bt2020));

    // ...while the connector stays SDR: engagement requires the active fullscreen window.
    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "an inactive fullscreen window must not engage HDR on the connector"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the output blend space must stay SDR while HDR is not engaged"
    );
}

#[test]
fn preferred_identities_are_stable() {
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);
    f.client(id).requery_preferred();
    f.double_roundtrip(id);

    let identities = &f.client(id).state.ready_identities;
    assert!(
        identities.len() >= 2,
        "expected two ready events, got {identities:?}"
    );
    let last_two = &identities[identities.len() - 2..];
    assert_eq!(
        last_two[0], last_two[1],
        "the same preferred description must keep the same identity"
    );
}

#[test]
fn hdr_description_on_subsurface_engages() {
    // winewayland (Proton) attaches the HDR image description to a Vulkan *subsurface* of the
    // toplevel, not the toplevel surface itself. Engagement must search the surface tree.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.set_fullscreen(None);
    window.ack_last_and_commit();
    f.double_roundtrip(id);
    let window = f.client(id).window(&surface);
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    // Vulkan-style subsurface carrying the HDR description.
    let subsurface = f.client(id).create_committed_subsurface(&surface);
    f.roundtrip(id);
    f.client(id).create_and_attach_hdr_description(
        &subsurface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    f.roundtrip(id);
    // The description is double-buffered; commit the subsurface and sync via the parent.
    let subsurface_clone = subsurface.clone();
    subsurface_clone.commit();
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let output = f.niri_output(1);
    let desc = f.niri().output_hdr_image_description(&output);
    assert!(
        desc.is_some_and(|d| d.is_hdr()),
        "HDR description on a subsurface must engage HDR, got {desc:?}"
    );
}

/// Sets up a fullscreen window with a committed buffer and returns its surface, the shared
/// preamble of the engagement tests.
fn fullscreen_window(f: &mut Fixture, id: ClientId) -> WlSurface {
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.set_fullscreen(None);
    window.ack_last_and_commit();
    f.double_roundtrip(id);
    let window = f.client(id).window(&surface);
    window.ack_last_and_commit();
    f.double_roundtrip(id);
    surface
}

#[test]
fn windows_scrgb_engages_hdr() {
    // winewayland in scRGB mode uses create_windows_scrgb rather than a parametric
    // description; it must become ready, attach, and engage HDR like PQ content.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    let ready_before = f.client(id).state.ready_identities.len();
    f.client(id).create_and_attach_scrgb_description(&surface);
    f.roundtrip(id);
    // The description is double-buffered surface state.
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let ready = &f.client(id).state.ready_identities;
    assert!(
        ready.len() > ready_before,
        "the scRGB image description must deliver ready, got {ready:?}"
    );

    let output = f.niri_output(1);
    let desc = f.niri().output_hdr_image_description(&output);
    assert!(
        desc.is_some_and(|d| d.windows_scrgb),
        "a fullscreen scRGB description must engage HDR with the flag intact, got {desc:?}"
    );
}

#[test]
fn windows_bt2100_engages_hdr() {
    // winewayland maps VK_COLOR_SPACE_HDR10_ST2084_EXT swapchains through the v3
    // create_windows_bt2100 request; the description must become ready, attach, and engage
    // HDR like other PQ content.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    let ready_before = f.client(id).state.ready_identities.len();
    f.client(id).create_and_attach_bt2100_description(&surface);
    f.roundtrip(id);
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let ready = &f.client(id).state.ready_identities;
    assert!(
        ready.len() > ready_before,
        "the BT.2100 image description must deliver ready, got {ready:?}"
    );

    let output = f.niri_output(1);
    let desc = f.niri().output_hdr_image_description(&output);
    assert!(
        desc.is_some_and(|d| d.windows_bt2100),
        "a fullscreen BT.2100 description must engage HDR with the flag intact, got {desc:?}"
    );
}

#[test]
fn output_description_luminances_satisfy_wine_hdr_check() {
    // winewayland only reports an HDR display to Windows when the output image description's
    // max target luminance exceeds its reference white — both read from the info events. Probe
    // the output like the wine driver does and evaluate its exact condition.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    f.double_roundtrip(id);

    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);

    let client = f.client(id);
    let (_, _, reference_lum) = client
        .state
        .info_luminances
        .expect("the luminances info event must be sent");
    let (_, max_target_lum) = client
        .state
        .info_target_luminance
        .expect("the target_luminance info event must be sent");
    // Reference white 203 cd/m² (default), max target = the injected EDID max of 800 cd/m².
    assert_eq!(reference_lum, 203);
    assert_eq!(max_target_lum, 800);
    assert!(
        max_target_lum > reference_lum,
        "wine's HDR display detection requires max target luminance > reference white"
    );
}

#[test]
fn auto_mode_advertises_hdr_capability_while_sdr() {
    // In auto mode with HDR not engaged, the output must still report its HDR capability:
    // winewayland reads "max target luminance > reference white" from these events once at
    // startup, before any fullscreen HDR content exists, and otherwise never enables its HDR
    // path. The capability rides on the *target* volume (which the spec allows to exceed the
    // primary volume), while the primary volume and transfer function stay honest about the
    // SDR connector: 1.0 is SDR white at the protocol's 80 cd/m² default.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    f.double_roundtrip(id);

    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);

    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::Srgb));
    assert_eq!(client.state.info_primaries, Some(Primaries::Srgb));
    // The honest sRGB primary volume defaults (0.2 / 80 / 80 cd/m²).
    assert_eq!(client.state.info_luminances, Some((2000, 80, 80)));
    // The injected EDID range as the target volume.
    assert_eq!(client.state.info_target_luminance, Some((100, 800)));
    let (_, max_target_lum) = client
        .state
        .info_target_luminance
        .expect("the target_luminance info event must be sent");
    assert!(
        max_target_lum > 80,
        "wine's HDR display detection must pass while auto mode is not engaged"
    );
}

#[test]
fn auto_active_fullscreen_hdr_engages() {
    // The engagement invariant of auto mode: the connector engages only while the *active*
    // fullscreen window carries HDR content.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    // Fullscreen but no HDR content yet: not engaged.
    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "a fullscreen window without HDR content must not engage HDR"
    );

    f.client(id).create_and_attach_hdr_description(
        &surface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "an active fullscreen HDR window must engage HDR in auto mode"
    );
    assert!(
        f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must be HDR while engaged"
    );
}

/// Attaches a PQ/BT.2020 description to `surface` and commits it.
fn attach_hdr_description(f: &mut Fixture, id: ClientId, surface: &WlSurface) {
    f.client(id).create_and_attach_hdr_description(
        surface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    f.client(id).window(surface).surface.commit();
    f.double_roundtrip(id);
}

/// Drops the color-management image description on `surface` and commits, so the window
/// stops carrying HDR content (no description means sRGB; the v2 protocol has no sRGB
/// name to attach).
fn unset_surface_description(f: &mut Fixture, id: ClientId, surface: &WlSurface) {
    f.client(id).unset_surface_image_description(surface);
    f.client(id).window(surface).surface.commit();
    f.double_roundtrip(id);
}

/// Replaces `surface`'s image description with a parametric SDR one (named TF and
/// primaries, no mastering metadata — the protocol rejects mastering fields with SDR
/// transfer functions) and commits.
fn attach_sdr_description(
    f: &mut Fixture,
    id: ClientId,
    surface: &WlSurface,
    tf: TransferFunction,
    primaries: Primaries,
) {
    f.client(id).create_and_attach_sdr_description(
        surface,
        tf,
        primaries,
        RenderIntent::Perceptual,
    );
    f.client(id).window(surface).surface.commit();
    f.double_roundtrip(id);
}

/// Takes `surface`'s window out of fullscreen. The unfullscreen lands only once the
/// client has acked the resulting configure and committed, and unfullscreening sends
/// exactly one configure, so this is a single ack-and-commit round after it arrives.
fn unfullscreen_window(f: &mut Fixture, id: ClientId, surface: &WlSurface) {
    f.client(id).window(surface).unset_fullscreen();
    f.double_roundtrip(id);
    f.client(id).window(surface).ack_last_and_commit();
    f.double_roundtrip(id);
}

#[test]
fn auto_hdr_disengages_on_leaving_fullscreen() {
    // Leaving fullscreen must drop the connector back to SDR (HDR that never reverts is a
    // known failure mode of auto-HDR implementations), and re-entering must engage it
    // again. The headless decision is immediate; the brief exit grace that rides out a
    // momentary description gap lives in the TTY backend and can only be verified on
    // hardware.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);
    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "precondition: engaged",
    );

    // Leave fullscreen.
    unfullscreen_window(&mut f, id, &surface);

    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "leaving fullscreen must disengage HDR"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must return to SDR after disengaging"
    );

    // Re-enter fullscreen: engages again. The fullscreen configure arrives after the
    // request, so let it land before acking it.
    f.client(id).window(&surface).set_fullscreen(None);
    f.double_roundtrip(id);
    f.client(id).window(&surface).ack_last_and_commit();
    f.double_roundtrip(id);

    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "re-entering fullscreen with HDR content must engage HDR again"
    );
}

#[test]
fn auto_hdr_disengages_on_losing_focus() {
    // Losing focus must disengage: engagement requires the *active* fullscreen window.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);
    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "precondition: engaged",
    );

    // A second window opens and takes focus. niri doesn't steal focus from an active
    // fullscreen window, so move focus explicitly (same dance as the inactive test).
    let id2 = f.add_client();
    let other = f.client(id2).create_window();
    let other_surface = other.surface.clone();
    other.commit();
    f.roundtrip(id2);
    let other = f.client(id2).window(&other_surface);
    other.attach_new_buffer();
    other.ack_last_and_commit();
    f.double_roundtrip(id2);
    f.niri().layout.focus_right();
    f.double_roundtrip(id2);

    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "losing focus must disengage HDR"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must return to SDR after losing focus"
    );
}

#[test]
fn auto_hdr_disengages_when_content_stops_being_hdr() {
    // The window stays fullscreen and focused, but drops its HDR description
    // (e.g. the game turns HDR off mid-game): the connector must disengage.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);
    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "precondition: engaged",
    );

    unset_surface_description(&mut f, id, &surface);

    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "an HDR description replaced by an sRGB one must disengage HDR"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must return to SDR when the content stops being HDR"
    );
}

#[test]
fn auto_hdr_disengages_when_content_switches_to_gamma22() {
    // A game that turns HDR off in its menu usually re-attaches a parametric SDR
    // description over the PQ one, rather than unsetting it: v2 names that TF gamma22
    // (the v1 srgb name is deprecated and hidden at v2). The connector must disengage.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);
    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "precondition: engaged",
    );

    attach_sdr_description(
        &mut f,
        id,
        &surface,
        TransferFunction::Gamma22,
        Primaries::Srgb,
    );

    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "a gamma22 description over the PQ one must disengage HDR"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must return to SDR when the content stops being HDR"
    );
}

#[test]
fn auto_windowed_hdr_content_does_not_engage() {
    // Windowed HDR content stays SDR: an active (focused) windowed window carrying an HDR
    // description must not engage the connector; the compositor renders it through the
    // existing HDR→SDR conversion path (pixel-level correctness of that path is
    // visual-test/hardware territory, the decision is what's checked here).
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);
    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "a windowed (not fullscreen) HDR window must not engage the connector"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "the blend space must stay SDR for windowed HDR content"
    );

    // And the client is told the output is SDR (capability on the target volume only).
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::Srgb));
    assert_eq!(client.state.info_luminances, Some((2000, 80, 80)));
}

#[test]
fn auto_advertised_target_volume_stable_across_engage() {
    // The advertised description does change on engage/disengage — the transfer function
    // must stay honest about what the connector expects — but everything winewayland
    // derives Windows' HDR state from (target luminance vs. reference white, and the EDID
    // fields: target primaries, MaxCLL/MaxFALL) is identical in both states. So its
    // supports_hdr flag and synthesized EDID don't change, and the image_description_changed
    // event that fires on the flip does not reconfigure the Windows display mid-game.
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let not_engaged = (
        f.client(id).state.info_luminances,
        f.client(id).state.info_target_luminance,
    );
    assert_eq!(not_engaged, (Some((2000, 80, 80)), Some((100, 800))));

    let changes_before = f.client(id).state.output_image_description_changed;

    attach_hdr_description(&mut f, id, &surface);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_some(),
        "precondition: engaged",
    );
    assert!(
        f.client(id).state.output_image_description_changed > changes_before,
        "engaging changes the advertised description, so clients must be notified"
    );

    // Re-query after the change: PQ now, EDID luminances, and the same target volume.
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::St2084Pq));
    assert_eq!(client.state.info_luminances, Some((100, 800, 203)));
    assert_eq!(
        client.state.info_target_luminance, not_engaged.1,
        "the target volume must not change across engage"
    );
    // Wine's check holds in the engaged state too: 800 > 203.
    let (_, max_target) = client.state.info_target_luminance.unwrap();
    assert!(max_target > 203);

    // Disengage (leave fullscreen) and check the flip back.
    let changes_mid = f.client(id).state.output_image_description_changed;
    unfullscreen_window(&mut f, id, &surface);
    assert!(
        f.client(id).state.output_image_description_changed > changes_mid,
        "disengaging must notify clients too"
    );

    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(
        (
            client.state.info_luminances,
            client.state.info_target_luminance
        ),
        not_engaged,
        "the description must return to exactly the not-engaged advertisement"
    );
}

#[test]
fn hdr_mode_change_emits_image_description_changed() {
    // Flipping the hdr mode at config reload must notify clients about the output whose
    // advertised description changed (capability appears/disappears).
    use niri_config::output::HdrMode;

    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    f.double_roundtrip(id);
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    assert_eq!(f.client(id).state.info_target_luminance, Some((100, 800)));

    let changes_before = f.client(id).state.output_image_description_changed;

    // auto → off.
    {
        let mut config = f.niri().config.borrow_mut();
        let output = config
            .outputs
            .0
            .iter_mut()
            .find(|o| o.name == "headless-1")
            .expect("fixture output");
        output.hdr = Some(Hdr {
            mode: Some(HdrMode::Off),
            ..Default::default()
        });
    }
    f.niri().refresh_color_management();
    f.double_roundtrip(id);

    assert!(
        f.client(id).state.output_image_description_changed > changes_before,
        "auto → off must emit image_description_changed"
    );
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(client.state.info_luminances, Some((2000, 80, 80)));
    assert_eq!(
        client.state.info_target_luminance,
        Some((2000, 80)),
        "an off output must not advertise an extended target volume"
    );

    // off → auto.
    let changes_mid = f.client(id).state.output_image_description_changed;
    {
        let mut config = f.niri().config.borrow_mut();
        let output = config
            .outputs
            .0
            .iter_mut()
            .find(|o| o.name == "headless-1")
            .expect("fixture output");
        output.hdr = Some(Hdr {
            mode: Some(HdrMode::Auto),
            ..Default::default()
        });
    }
    f.niri().refresh_color_management();
    f.double_roundtrip(id);

    assert!(
        f.client(id).state.output_image_description_changed > changes_mid,
        "off → auto must emit image_description_changed"
    );
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    assert_eq!(f.client(id).state.info_target_luminance, Some((100, 800)));
}

#[test]
fn off_mode_never_engages() {
    // `hdr mode="off"` (the default): even an active fullscreen window with an HDR
    // description must not engage the connector; the output advertises plain SDR and no
    // window is ever told to prefer PQ.
    use niri_config::output::HdrMode;

    let mut config = Config::default();
    config.outputs.0.push(Output {
        name: "headless-1".to_owned(),
        hdr: Some(Hdr {
            mode: Some(HdrMode::Off),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1920, 1080));
    f.niri_output(1)
        .user_data()
        .insert_if_missing(|| OutputHdrCaps {
            supported: true,
            max_luminance: 800,
            min_luminance: 100,
            max_frame_avg_luminance: 600,
        });

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);
    f.client(id).create_and_attach_hdr_description(
        &surface,
        TransferFunction::St2084Pq,
        Primaries::Bt2020,
        RenderIntent::Perceptual,
    );
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let output = f.niri_output(1);
    assert!(
        f.niri().output_hdr_image_description(&output).is_none(),
        "mode off must never engage HDR"
    );
    assert!(
        !f.niri().output_blend_description(&output).is_hdr(),
        "mode off must keep the blend space SDR"
    );

    // The client is told plain SDR — no capability claim.
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);
    let client = f.client(id);
    assert_eq!(client.state.info_tf, Some(TransferFunction::Srgb));
    assert_eq!(client.state.info_luminances, Some((2000, 80, 80)));

    // And the fullscreen window is not told to prefer PQ.
    f.client(id).probe_surface_preferred(&surface);
    f.double_roundtrip(id);
    assert_eq!(f.client(id).state.info_tf, Some(TransferFunction::Srgb));
}

#[test]
fn extended_target_volume_roundtrip() {
    use smithay::wayland::color::management::{Chromaticities, Primaries as ServerPrimaries};

    // A target color volume (BT.2020 mastering primaries) exceeding the container primaries
    // (sRGB) requires the advertised extended_target_volume feature; without it, smithay
    // fails the description gracefully and set_image_description would raise a protocol
    // error. The mastering primaries must round-trip into the engaged description, where the
    // TTY backend forwards them as ST 2086 metadata.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    f.client(id)
        .create_and_attach_hdr_description_with_target_volume(
            &surface,
            TransferFunction::St2084Pq,
            Primaries::Srgb,
            RenderIntent::Perceptual,
        );
    f.roundtrip(id);
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let output = f.niri_output(1);
    let desc = f
        .niri()
        .output_hdr_image_description(&output)
        .expect("PQ description must engage HDR");
    assert_eq!(
        desc.mastering_primaries,
        Some(Chromaticities::from_named(ServerPrimaries::Bt2020)),
        "client mastering display primaries must be preserved"
    );
    assert_eq!(desc.primaries.named, Some(ServerPrimaries::Srgb));
}

#[test]
fn custom_primaries_description() {
    use smithay::wayland::color::management::Chromaticities;

    // A set_primaries client provides raw chromaticity coordinates instead of a named set;
    // the description must be created (the feature is advertised) and the raw values must be
    // visible server-side, with no named primaries attached.
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    let surface = fullscreen_window(&mut f, id);

    // Display P3 coordinates, in protocol wire units (xy * 1e6).
    let p3 = [
        (680_000, 320_000),
        (265_000, 690_000),
        (150_000, 60_000),
        (312_700, 329_000),
    ];
    f.client(id).create_and_attach_custom_primaries_description(
        &surface,
        TransferFunction::St2084Pq,
        p3,
        RenderIntent::Perceptual,
    );
    f.roundtrip(id);
    f.client(id).window(&surface).surface.commit();
    f.double_roundtrip(id);

    let output = f.niri_output(1);
    let desc = f
        .niri()
        .output_hdr_image_description(&output)
        .expect("PQ description must engage HDR");
    assert_eq!(desc.primaries.named, None);
    assert_eq!(
        desc.primaries.values,
        Some(Chromaticities {
            red: p3[0],
            green: p3[1],
            blue: p3[2],
            white: p3[3],
        })
    );
}
