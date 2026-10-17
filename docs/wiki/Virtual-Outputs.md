# Virtual Outputs

Virtual outputs are extra `wl_output`s that niri can create even when there’s no physical monitor
attached (or in addition to real monitors). By default they’re named `HEADLESS-1`, `HEADLESS-2`,
etc., or you can give them your own name.

They’re useful for:

- Sunshine / Moonlight (wlr-screencopy capture)
- VNC (e.g. wayvnc)
- “headless” remote sessions and general screen sharing

Virtual outputs work on the TTY and Headless backends (not the Winit backend).

## Creating virtual outputs

```bash
# Create a 1920x1080@144 virtual output named "sunshine"
niri msg create-virtual-output --name sunshine --mode 1920x1080@144

# With a scale factor:
niri msg create-virtual-output --name sunshine --mode 1920x1080@144 --scale 1.25
```

- `--mode` is `WIDTHxHEIGHT[@REFRESH]`; the refresh rate is optional and defaults to 60 Hz.
  If `--mode` is omitted entirely, the default is `1920x1080@60`.
- If `--name` is omitted, a unique `HEADLESS-N` name is generated.
- If an output with the given name already exists and is virtual, it is **resized in place**
  instead of creating a new one (re-running the command with a new mode is the intended way to
  change a live virtual output’s mode; `niri msg output NAME mode …` works too).
- Modes are validated: dimensions must be 16..=16384 per side, refresh rate > 0 and ≤ 1000 Hz.

To see what exists, use `niri msg outputs` — virtual outputs show up there with their real name
(`make` is `niri`, `model` is the output name).

### Headless backend (no physical displays)

For servers or remote-only access (e.g. SSH) where a real TTY is not available:

```bash
# Start niri in headless mode
NIRI_BACKEND=headless niri --session &
# A 1920x1080@60 HEADLESS-1 virtual output is created by default
```

Note: if you’re running commands from another shell, you may need to set `WAYLAND_DISPLAY` to the
socket of that headless session.

On machines with multiple GPUs, the headless backend’s EGL renderer can end up on software
rendering, which breaks dmabuf capture. Point niri at the render node you want:

```kdl
debug {
    render-drm-device "/dev/dri/renderD128"
}
```

## Removing virtual outputs

```bash
niri msg remove-virtual-output sunshine
```

Outputs declared with `create-virtual` in the config cannot be removed over IPC while they’re in
the config — remove or disable them there instead.

## Configuring virtual outputs

Virtual outputs can be configured like regular outputs:

```bash
# Enable/disable (off disconnects the output from clients but keeps it ready)
niri msg output sunshine <on|off>

# Set scale
niri msg output sunshine scale 1.25

# Set transform
niri msg output sunshine transform 90

# Change the mode (same custom mode as create-virtual-output --mode)
niri msg output sunshine custom-mode 2560x1440@120
```

These are temporary (like all `niri msg output` changes) and are forgotten on the next config
reload. Mode changes while the output is `off` are applied when it’s enabled again.

You can also configure them in your `config.kdl` file. If you want niri to *create* a named
virtual output automatically (at compositor startup and on config reload), add the
`create-virtual` flag to the corresponding `output` block:

```kdl
output "sunshine" {
    create-virtual
    mode "1920x1080@144.000"
    scale 1.25
    transform "90"
    position x=1920 y=0
}
```

## Screen-capture behavior

Virtual outputs are rendered on a synthetic frame clock that ticks at the output’s refresh rate,
independent of any capture client. Capture is never throttled below the mode’s refresh rate.

Capture is zero-copy where the client uses dmabufs: Sunshine’s wlgrab allocates dmabuf buffers on
the render node niri advertises via linux-dmabuf feedback, and niri renders directly into them.
Clients that pass SHM buffers (e.g. `grim` by default) get the normal SHM path.

The portal (xdg-desktop-portal-gnome / -gtk) shows virtual outputs by their real name: the
display name is `"{make} {model}"`, i.e. “niri sunshine” for an output named `sunshine`.

## Using with Sunshine

[Sunshine](https://github.com/LizardByte/Sunshine) can create a virtual output that matches the
client ([Moonlight](https://github.com/moonlight-stream/moonlight-qt)) resolution + refresh rate
when a session starts, and remove it when the session ends. Sunshine exposes
`SUNSHINE_CLIENT_WIDTH`, `SUNSHINE_CLIENT_HEIGHT` and `SUNSHINE_CLIENT_FPS` for this.

In `sunshine.conf`, use the wlr capture backend and a global prep command:

```ini
capture = wlr
output_name = sunshine

global_prep_cmd = [{"do":"sh -c \"niri msg create-virtual-output --name sunshine --mode ${SUNSHINE_CLIENT_WIDTH}x${SUNSHINE_CLIENT_HEIGHT}@${SUNSHINE_CLIENT_FPS}\"","undo":"niri msg remove-virtual-output sunshine"}]
```

Because `create-virtual-output` resizes an existing output in place, the `do` command is safe to
re-run when a client connects with a different resolution.

Alternatively, keep it per-app in `apps.json`:

```json
{
  "apps": [
    {
      "name": "Remote Desktop",
      "output": "sunshine",
      "prep-cmd": [
        {
          "do": "sh -c \"niri msg create-virtual-output --name sunshine --mode ${SUNSHINE_CLIENT_WIDTH}x${SUNSHINE_CLIENT_HEIGHT}@${SUNSHINE_CLIENT_FPS}\"",
          "undo": "niri msg remove-virtual-output sunshine"
        }
      ]
    }
  ]
}
```

### Headless backend (reuse the default output)

In headless mode, niri creates `HEADLESS-1` by default. You can just change its mode for the
session and restore it afterwards.

```json
{
  "apps": [
    {
      "name": "Remote Desktop",
      "output": "HEADLESS-1",
      "prep-cmd": [
        {
          "do": "sh -c \"niri msg output HEADLESS-1 custom-mode \\\"${SUNSHINE_CLIENT_WIDTH}x${SUNSHINE_CLIENT_HEIGHT}@${SUNSHINE_CLIENT_FPS}\\\"\"",
          "undo": "niri msg output HEADLESS-1 mode '1920x1080@60.000'"
        }
      ]
    }
  ]
}
```

## Input and seats (headless mode)

In headless mode, niri can still use libinput to read local input devices (if it has permission to
open `/dev/input/event*`). This is independent of virtual outputs.

libinput enumerates devices by udev seat. By default niri uses `seat0`, but you can override it:

```bash
XDG_SEAT=seat0 NIRI_BACKEND=headless niri --session
```

The Wayland `wl_seat` name exposed by niri matches this seat string (`XDG_SEAT` / `seat0`).
If niri can’t access any input devices (permissions, container, etc.), it will still start; you’ll
just have no _local_ input.

This section is mostly about _local_ kernel input devices. If you’re using a remote client that
injects input over Wayland (like wayvnc), niri doesn’t need access to `/dev/input` for that.

## Using with wayvnc

[wayvnc](https://github.com/any1/wayvnc) is a VNC server for wlroots-based Wayland compositors. It
forwards keyboard and pointer input from VNC clients to the compositor using Wayland “virtual
input” protocols (virtual keyboard + virtual pointer). This means you can have working remote input
even if the headless niri process can’t open `/dev/input/event*`.

### Physical displays + extra virtual output

```bash
# 1. Start niri normally on your TTY
niri --session &

# 2. Create a virtual output for VNC
niri msg create-virtual-output --mode 1920x1080

# 3. Start wayvnc on the virtual output
wayvnc --output HEADLESS-1

# 4. Connect from a VNC client to your machine's IP
```

### Pure headless (remote only)

```bash
# 1. Start niri in headless mode (e.g., over SSH)
NIRI_BACKEND=headless niri &

# 2. Start wayvnc
WAYLAND_DISPLAY=wayland-1 wayvnc --output HEADLESS-1

# 3. Connect from a VNC client
```

### Headless with systemd

For a persistent headless niri session:

```ini
# ~/.config/systemd/user/niri-headless.service
[Unit]
Description=Niri Headless Session

[Service]
Type=simple
Environment=NIRI_BACKEND=headless
ExecStart=/usr/bin/niri
Restart=on-failure

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now niri-headless
```
