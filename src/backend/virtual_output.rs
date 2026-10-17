use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use niri_config::{Config, OutputName};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::utils::Size;

use super::{IpcOutputMap, OutputId, VirtualOutputMarker};
use crate::frame_clock::FrameClock;
use crate::niri::Niri;
use crate::utils::logical_output;

/// Smallest and largest allowed virtual output dimension, in pixels.
///
/// The upper bound matches the common GBM maximum texture size, so a virtual output can always be
/// captured into a client-allocated dmabuf on the render GPU.
const MIN_VIRTUAL_DIM: u16 = 16;
const MAX_VIRTUAL_DIM: u16 = 16384;

/// Largest allowed virtual output refresh rate, in hertz.
const MAX_VIRTUAL_REFRESH_HZ: u32 = 1000;

/// Validates a virtual output mode.
///
/// `refresh_millihz` is the refresh rate in millihertz. Returns an error string suitable for
/// returning straight to an IPC client.
pub(super) fn validate_virtual_mode(
    width: u16,
    height: u16,
    refresh_millihz: u32,
) -> Result<(), String> {
    if !(MIN_VIRTUAL_DIM..=MAX_VIRTUAL_DIM).contains(&width) {
        return Err(format!(
            "width {width} is out of range {MIN_VIRTUAL_DIM}..={MAX_VIRTUAL_DIM}"
        ));
    }

    if !(MIN_VIRTUAL_DIM..=MAX_VIRTUAL_DIM).contains(&height) {
        return Err(format!(
            "height {height} is out of range {MIN_VIRTUAL_DIM}..={MAX_VIRTUAL_DIM}"
        ));
    }

    if refresh_millihz == 0 || refresh_millihz > MAX_VIRTUAL_REFRESH_HZ * 1000 {
        return Err(format!(
            "refresh rate must be greater than 0 and at most {MAX_VIRTUAL_REFRESH_HZ} Hz"
        ));
    }

    Ok(())
}

/// Validates a user-provided virtual output name.
pub(super) fn validate_virtual_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("virtual output name cannot be empty".to_string());
    }

    if name.len() > 64 {
        return Err("virtual output name is too long (max 64 bytes)".to_string());
    }

    if name.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(
            "virtual output name cannot contain whitespace or control characters".to_string(),
        );
    }

    Ok(())
}

pub(super) struct BuiltVirtualOutput {
    pub name: String,
    pub output: Output,
    pub output_id: OutputId,
    pub refresh_interval: Duration,
    pub ipc_output: niri_ipc::Output,
}

pub(super) fn refresh_interval_from_millihz(refresh_millihz: u64) -> Duration {
    let refresh_millihz = refresh_millihz.max(1);
    let interval_nanos = 1_000_000_000_000 / refresh_millihz;

    // Clamp extremely low refresh rates to a reasonable pacing interval.
    // This matches existing virtual-output code paths (off/on + mode changes).
    if interval_nanos >= 1_000_000_000 {
        Duration::from_micros(16_667)
    } else {
        Duration::from_nanos(interval_nanos)
    }
}

fn set_virtual_output_mode_single(output: &Output, mode: Mode) {
    // Virtual outputs represent a single synthetic mode.
    //
    // Smithay's `Output` stores a monotonically-growing list of modes. That'd be correct for real
    // monitors, but for virtual outputs it confuses some clients (e.g. Sunshine/Moonlight) after
    // multiple custom-mode cycles. Prune the mode list back down to only the active mode so new
    // wl_output bindings don't see stale historical modes.
    //
    // Also, set the preferred mode before changing the current mode so the emitted wl_output
    // `mode` event includes both CURRENT and PREFERRED flags.
    output.set_preferred(mode);
    output.change_current_state(Some(mode), None, None, None);

    for other in output.modes() {
        if other != mode {
            output.delete_mode(other);
        }
    }
}

fn build_virtual_output(
    counter: &mut u32,
    width: u16,
    height: u16,
    refresh_millihz: u32,
    name: Option<String>,
) -> BuiltVirtualOutput {
    let refresh_millihz = refresh_millihz.clamp(1, i32::MAX as u32);
    let refresh_millihz = refresh_millihz as i32;

    let is_unnamed = name.is_none();
    let n = if is_unnamed {
        *counter += 1;
        *counter
    } else {
        *counter
    };

    let serial = n.to_string();
    let connector = name.unwrap_or_else(|| format!("HEADLESS-{n}"));
    let model = connector.clone();
    let make = "niri".to_string();

    let output = Output::new(
        connector.clone(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: make.clone(),
            model: model.clone(),
            serial_number: serial.clone(),
        },
    );

    let mode = Mode {
        size: Size::from((i32::from(width), i32::from(height))),
        refresh: refresh_millihz,
    };
    set_virtual_output_mode_single(&output, mode);

    output.user_data().insert_if_missing(|| OutputName {
        connector: connector.clone(),
        make: Some(make),
        model: Some(model),
        serial: Some(serial),
    });

    output
        .user_data()
        .insert_if_missing(VirtualOutputMarker::default);

    let output_id = OutputId::next();

    let physical_properties = output.physical_properties();
    let ipc_output = niri_ipc::Output {
        name: output.name(),
        make: physical_properties.make,
        model: physical_properties.model,
        serial: None,
        physical_size: None,
        modes: vec![niri_ipc::Mode {
            width,
            height,
            refresh_rate: refresh_millihz as u32,
            is_preferred: true,
        }],
        current_mode: Some(0),
        is_custom_mode: true,
        vrr_supported: false,
        vrr_enabled: false,
        logical: Some(logical_output(&output)),
        max_bpc: None,
    };

    let refresh_interval = refresh_interval_from_millihz(refresh_millihz as u64);

    BuiltVirtualOutput {
        name: connector,
        output,
        output_id,
        refresh_interval,
        ipc_output,
    }
}

/// Manages the set of virtual outputs owned by a backend.
///
/// Shared by the TTY and headless backends: it tracks the outputs by name, allocates unique
/// `HEADLESS-N` names, and applies config (mode / off / on) to them.
#[derive(Default)]
pub struct VirtualOutputs {
    /// Counter for auto-naming outputs (HEADLESS-1, HEADLESS-2, etc.)
    counter: u32,
    /// Virtual outputs indexed by output name, including currently-off ones.
    outputs: HashMap<String, (Output, OutputId)>,
}

impl VirtualOutputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// An iterator over all managed virtual outputs (including off ones).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &(Output, OutputId))> {
        self.outputs.iter()
    }

    /// Create a virtual output and add it to the compositor.
    ///
    /// If `name` matches an existing output config, the configured mode and `off` state take
    /// precedence over the requested `width`/`height`/`refresh_millihz`. Returns the name of the
    /// created output.
    pub fn create(
        &mut self,
        niri: &mut Niri,
        ipc_outputs: &Arc<Mutex<IpcOutputMap>>,
        width: u16,
        height: u16,
        refresh_millihz: u32,
        name: Option<String>,
    ) -> Result<String, String> {
        let mut width = width;
        let mut height = height;
        let mut refresh_millihz = refresh_millihz;
        let mut should_connect = true;

        if let Some(name) = name.as_deref() {
            validate_virtual_name(name)?;

            let lookup_name = OutputName {
                connector: name.to_string(),
                make: None,
                model: None,
                serial: None,
            };
            let config = niri.config.borrow();
            if let Some(output_config) = config.outputs.find(&lookup_name) {
                should_connect = !output_config.off;

                if let Some(mode_config) = output_config.mode {
                    width = mode_config.mode.width;
                    height = mode_config.mode.height;

                    let refresh_hz = mode_config.mode.refresh.unwrap_or(60.0);
                    refresh_millihz =
                        (refresh_hz * 1000.0).round().clamp(1.0, u32::MAX as f64) as u32;
                }
            }

            if self.outputs.contains_key(name)
                || niri.global_space.outputs().any(|o| o.name() == name)
            {
                return Err(format!("output '{name}' already exists"));
            }
        }

        validate_virtual_mode(width, height, refresh_millihz)?;

        let built = build_virtual_output(&mut self.counter, width, height, refresh_millihz, name);

        if self.outputs.contains_key(&built.name)
            || niri.global_space.outputs().any(|o| o.name() == built.name)
        {
            return Err(format!("output '{}' already exists", built.name));
        }

        ipc_outputs
            .lock()
            .unwrap()
            .insert(built.output_id, built.ipc_output);

        self.outputs
            .insert(built.name.clone(), (built.output.clone(), built.output_id));

        if should_connect {
            niri.add_output(built.output, Some(built.refresh_interval), false);
        }

        Ok(built.name)
    }

    /// Remove the virtual output with the given name.
    ///
    /// Refuses to remove an output that is declared with `create-virtual` in the config, so the
    /// config stays the owner of such outputs.
    pub fn remove(
        &mut self,
        niri: &mut Niri,
        ipc_outputs: &Arc<Mutex<IpcOutputMap>>,
        name: &str,
    ) -> Result<(), String> {
        let lookup_name = OutputName {
            connector: name.to_string(),
            make: None,
            model: None,
            serial: None,
        };

        if niri
            .config
            .borrow()
            .outputs
            .find(&lookup_name)
            .is_some_and(|output| output.create_virtual)
        {
            return Err(format!(
                "output '{name}' is configured; remove it from the config or disable it there instead"
            ));
        }

        let (output, output_id) = self
            .outputs
            .remove(name)
            .ok_or_else(|| format!("virtual output '{name}' not found"))?;

        ipc_outputs.lock().unwrap().remove(&output_id);

        if niri.output_exists(&output) {
            niri.remove_output(&output);
        }

        Ok(())
    }

    /// Apply output config (mode, off/on) to all managed virtual outputs.
    ///
    /// When `ipc_outputs` is `Some`, the IPC entries are kept in sync with the new mode and
    /// connection state. The TTY backend passes `None` because it rebuilds its whole IPC map
    /// separately via `refresh_ipc_outputs`.
    pub fn apply_config(
        &mut self,
        niri: &mut Niri,
        ipc_outputs: Option<&Arc<Mutex<IpcOutputMap>>>,
        config: &Rc<RefCell<Config>>,
    ) {
        let config = config.borrow();
        let mut resized_outputs = Vec::new();

        // Apply config to all managed virtual outputs, even if currently disconnected (off).
        // This allows `off`/`on` to work without losing internal state or IPC entries.
        for (output_name, (output, output_id)) in self.outputs.iter_mut() {
            let name = output.user_data().get::<OutputName>().unwrap();
            let output_config = config.outputs.find(name);

            let is_off = output_config.is_some_and(|c| c.off);
            let new_mode = output_config.and_then(|config| {
                config.mode.as_ref().map(|mode_config| {
                    let refresh_hz = mode_config.mode.refresh.unwrap_or(60.0);
                    let refresh_millihz =
                        (refresh_hz * 1000.0).round().clamp(1.0, i32::MAX as f64) as i32;
                    Mode {
                        size: Size::from((
                            i32::from(mode_config.mode.width),
                            i32::from(mode_config.mode.height),
                        )),
                        refresh: refresh_millihz,
                    }
                })
            });

            // Apply mode changes even when off so the mode is ready when re-enabled.
            let mut mode_changed = false;
            if let Some(new_mode) = new_mode {
                if output.current_mode() != Some(new_mode) {
                    set_virtual_output_mode_single(output, new_mode);
                    mode_changed = true;

                    if let Some(ipc_outputs) = ipc_outputs {
                        if let Some(ipc_output) = ipc_outputs.lock().unwrap().get_mut(output_id) {
                            ipc_output.modes = vec![niri_ipc::Mode {
                                width: new_mode.size.w as u16,
                                height: new_mode.size.h as u16,
                                refresh_rate: new_mode.refresh as u32,
                                is_preferred: true,
                            }];
                            ipc_output.current_mode = Some(0);
                            ipc_output.is_custom_mode = true;
                        }
                    }
                }
            }

            let was_connected = niri
                .global_space
                .outputs()
                .any(|o| o.name() == *output_name);

            // Handle off/on by removing/adding the output from/to the space.
            match (is_off, was_connected) {
                (true, true) => {
                    niri.remove_output(output);
                    niri.ipc_outputs_changed = true;
                }
                (false, false) => {
                    let refresh_millihz = output
                        .current_mode()
                        .map(|m| m.refresh.max(1) as u64)
                        .unwrap_or(60_000);
                    let refresh_interval = refresh_interval_from_millihz(refresh_millihz);
                    niri.add_output(output.clone(), Some(refresh_interval), false);
                    niri.ipc_outputs_changed = true;
                    resized_outputs.push(output.clone());
                }
                _ => {}
            }

            let is_connected = niri
                .global_space
                .outputs()
                .any(|o| o.name() == *output_name);

            if mode_changed && !is_off && is_connected {
                if let Some(new_mode) = output.current_mode() {
                    // Keep refresh pacing in sync with the new mode when connected.
                    if let Some(output_state) = niri.output_state.get_mut(output) {
                        let refresh_millihz = new_mode.refresh.max(1) as u64;
                        let refresh_interval = refresh_interval_from_millihz(refresh_millihz);
                        output_state.frame_clock = FrameClock::new(Some(refresh_interval), false);
                    }
                }

                resized_outputs.push(output.clone());
            }

            if let Some(ipc_outputs) = ipc_outputs {
                if let Some(ipc_output) = ipc_outputs.lock().unwrap().get_mut(output_id) {
                    ipc_output.logical = is_connected.then(|| logical_output(output));
                }
            }
        }

        if !resized_outputs.is_empty() {
            niri.ipc_outputs_changed = true;
            for output in resized_outputs {
                if niri.output_state.contains_key(&output) {
                    niri.output_resized(&output);
                    niri.queue_redraw(&output);
                }
            }
        }
    }
}
