//! Tests for the ext-image-copy-capture offering: session creation, buffer constraints and the
//! color contract between captured pixels and the source output's
//! `wp_color_management_output_v1` image description. The headless fixture renders with EGL
//! surfaceless (software), so these cover session creation, the constraints handshake and the
//! blend-space decision, not the actual pixels (dmabuf formats depend on the render node and can
//! only be checked against real hardware).

use niri_config::output::{Hdr, HdrMode};
use niri_config::{Config, Output};
use smithay::reexports::wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::{
    Primaries, TransferFunction,
};

use wayland_client::protocol::wl_shm;

use super::*;
use crate::backend::OutputHdrCaps;
use crate::render_helpers::CaptureBlend;

/// Fixture whose output is `hdr mode="on"` with backend HDR capabilities injected, simulating an
/// HDR-capable monitor on the TTY backend.
fn fixture_with_hdr_mode_on() -> Fixture {
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

/// Fixture with `hdr mode="auto"` and injected capabilities, but no HDR content: the color
/// manager is advertised, yet the output blend description stays SRGB.
fn fixture_with_hdr_mode_auto() -> Fixture {
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
fn output_session_headless_gets_constraints() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));

    let id = f.add_client();
    f.double_roundtrip(id);

    assert!(
        f.client(id).state.output_capture_source_manager.is_some(),
        "ext_output_image_capture_source_manager_v1 was not advertised/bound"
    );
    assert!(
        f.client(id).state.image_copy_capture_manager.is_some(),
        "ext_image_copy_capture_manager_v1 was not advertised/bound"
    );

    let idx = f.client(id).create_output_capture_session();
    f.double_roundtrip(id);

    // The headless backend normally renders with EGL surfaceless, so niri builds the
    // constraints inside `with_primary_renderer`: the session gets the output size and an
    // Xrgb8888 shm format. Whether dmabuf formats are offered depends on the renderer having
    // a DRM render node, which the surfaceless display usually doesn't; what a real renderer
    // offers is hardware-only territory. If the environment has no EGL device at all, the
    // session legitimately gets no constraints — that's the environment, not the contract,
    // so bail out with a clear message instead of failing on an unrelated assertion.
    let has_renderer = f
        .niri_state()
        .backend
        .with_primary_renderer(|_| ())
        .is_some();

    let constraints = &f.client(id).state.capture_constraints[idx];

    if !has_renderer {
        eprintln!(
            "no primary renderer in this environment: skipping the constraint assertions \
             for session {idx}"
        );
        assert_eq!(
            constraints.done, 0,
            "without a renderer the session must not receive constraints"
        );
        return;
    }

    assert_eq!(constraints.done, 1);
    assert_eq!(constraints.size, Some((1920, 1080)));
    assert_eq!(
        constraints.shm_formats,
        vec![wl_shm::Format::Xrgb8888 as u32]
    );
}

#[test]
fn capture_blend_is_sdr_without_hdr() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));

    let output = f.niri_output(1);
    assert_eq!(
        f.niri().output_capture_blend(&output),
        CaptureBlend::Sdr {
            reference_luminance: 203.
        }
    );
}

#[test]
fn capture_blend_is_sdr_in_auto_mode_without_hdr_content() {
    let mut f = fixture_with_hdr_mode_auto();

    let output = f.niri_output(1);
    assert_eq!(
        f.niri().output_capture_blend(&output),
        CaptureBlend::Sdr {
            reference_luminance: 203.
        }
    );
}

#[test]
fn capture_blend_is_pq_when_hdr_on() {
    let mut f = fixture_with_hdr_mode_on();

    let output = f.niri_output(1);
    // Default SDR reference white, matching hdr_blend_description().
    assert_eq!(
        f.niri().output_capture_blend(&output),
        CaptureBlend::Pq {
            reference_luminance: 203.
        }
    );
}

#[test]
fn output_image_description_reports_pq_when_hdr_on() {
    let mut f = fixture_with_hdr_mode_on();

    let id = f.add_client();
    f.double_roundtrip(id);
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);

    let state = &f.client(id).state;
    assert_eq!(state.info_tf, Some(TransferFunction::St2084Pq));
    assert_eq!(state.info_primaries, Some(Primaries::Bt2020));
    assert_eq!(
        state.info_luminances.map(|(_, _, reference)| reference),
        Some(203)
    );
}

#[test]
fn output_image_description_reports_srgb_when_not_engaged() {
    let mut f = fixture_with_hdr_mode_auto();

    let id = f.add_client();
    f.double_roundtrip(id);
    f.client(id).probe_output_color_management();
    f.double_roundtrip(id);

    let state = &f.client(id).state;
    assert_eq!(state.info_tf, Some(TransferFunction::Srgb));
    assert_eq!(state.info_primaries, Some(Primaries::Srgb));
    // In auto mode the primary volume stays at the honest sRGB defaults while the connector
    // is SDR; the HDR capability rides on the target volume (target max > reference is what
    // Wine's DXGI backend reads as "HDR capable").
    assert_eq!(state.info_luminances, Some((2000, 80, 80)));
    assert_eq!(state.info_target_luminance, Some((100, 800)));
}
