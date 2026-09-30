# niri, HDR branch (`erikg-hdr`)

> **Built with LLM assistance.** The features and fixes I add in this fork are written with the help of large language models, specifically Qwen 3.8 Flash-Next. niri doesn't accept LLM-assisted contributions, so I will never submit them to [niri-wm/niri](https://github.com/niri-wm/niri); they stay in this fork. dividebysandwich's HDR work and the commits picked in from niri pull requests are their authors' own.

This is my personal build of [niri](https://github.com/niri-wm/niri), the scrollable-tiling Wayland compositor. I run it on my ASUS ProArt PX13 laptop (AMD Radeon 8060S) and my desktop (NVIDIA RTX 5070 Ti). It is not an official niri release and is not affiliated with the niri project.

## Where it comes from

```
niri-wm/niri                    upstream niri (no HDR yet)
└── dividebysandwich/niri       hdr-smithay-master: HDR through Smithay color management
    └── gonzalezerik/niri       erikg-hdr (this branch): the fixes and extras below
```

Upstream niri doesn't support HDR yet; that depends on color management landing in [Smithay](https://github.com/Smithay/smithay). Until then, this branch builds on [dividebysandwich/niri](https://github.com/dividebysandwich/niri)'s HDR work.

## What this branch adds

**HDR fixes**, each also submitted to dividebysandwich/niri:

| Fix | Pull request |
|---|---|
| Don't free the swapchain on blend-space changes (AMD GPU page fault after leaving HDR) | [#4](https://github.com/dividebysandwich/niri/pull/4) |
| Wait for the 10-bit probe frame before discarding its buffers | [#5](https://github.com/dividebysandwich/niri/pull/5) |
| Ride out a momentary loss of the HDR description (HDR → SDR → HDR blanking when mpv goes fullscreen) | [#6](https://github.com/dividebysandwich/niri/pull/6) |
| Prefer HDR for a window as soon as it is told to go fullscreen (games in nested gamescope starting in SDR) | [#7](https://github.com/dividebysandwich/niri/pull/7) |

**Per-output color profiles** (only in this branch): color correction from an ICC profile or the display's EDID, applied in the GPU's display pipeline, similar to KDE Plasma's "Color profile" setting. See [Configuration: Color Profiles](docs/wiki/Configuration:-Color-Profiles.md).

**Auto-rotate** (only in this branch): rotates an output from the accelerometer through iio-sensor-proxy, for 2-in-1 laptops like the PX13. Carried from [niri-wm/niri#3763](https://github.com/niri-wm/niri/pull/3763) by Rio Sanjaya ([@oisanjaya](https://github.com/oisanjaya)), squashed into one commit; all credit to them. It is an opt-in build feature, so desktops don't run the accelerometer watcher: build with `cargo build --release --features auto-rotate`.

**Float above fullscreen** (until upstream merges it): the `float-above-fullscreen` window rule keeps a floating window visible over a focused fullscreen window, e.g. a picture-in-picture video over a game. Carried from [niri-wm/niri#4062](https://github.com/niri-wm/niri/pull/4062) by Zhai Can ([@bczhc](https://github.com/bczhc)), with the original commits.

**HDR capture** (only in this branch): captured dmabuf buffers hold the output's own encoding — PQ/BT.2020 while the output's `wp_color_management_output_v1` image description says PQ, sRGB otherwise — so HDR streams (e.g. Sunshine) get correct pixels through the existing color-management protocol, no new protocol. The buffer format is a precision choice only: pick a 10-bit (e.g. `XR30`) or FP16 format for HDR captures to avoid banding. shm capture stays the SDR tone-map of the output, since shm buffers cannot carry a color description. wlr-screencopy (and thus PipeWire) dmabuf buffers follow the same contract.

## Using it

Build it the same way as upstream niri. Smithay comes from [gonzalezerik/smithay](https://github.com/gonzalezerik/smithay) `erikg-hdr-smithay` (my fork of [dividebysandwich/smithay](https://github.com/dividebysandwich/smithay) `ds-hdr-master`), pinned in `Cargo.toml`, so no local Smithay checkout is needed.

```kdl
output "eDP-1" {
    hdr mode="auto"   // HDR while a fullscreen app shows HDR content
    auto-rotate       // follow the accelerometer (needs --features auto-rotate and iio-sensor-proxy)
}

color-profile "eDP-1" {
    source "icc"
    icc "~/.local/share/icc/panel.icm"
}
```

- HDR options: [Configuration: Outputs, `hdr`](docs/wiki/Configuration:-Outputs.md#hdr). HDR is experimental: it is limited to a single fullscreen application, and screenshots of HDR content come out in SDR.
- Color profiles: [Configuration: Color Profiles](docs/wiki/Configuration:-Color-Profiles.md).

## Bugs encountered

Bugs I hit while running this branch on my two machines, in the compositor and in the apps used with it. I don't open pull requests on niri or comment on upstream issues from this fork; this is the record.

### HDR, gaming and video

| Bug | In | Upstream issue | Fix |
|---|---|---|---|
| The branch didn't build on its own: `Cargo.toml` pointed Smithay at a local `../smithay` checkout | dividebysandwich/niri | none (issues are disabled on that repository) | Pinned to `dividebysandwich/smithay` `ds-hdr-master` ([6fa1bda3](https://github.com/gonzalezerik/niri/commit/6fa1bda3)) |
| AMD GPU page fault, freezing the desktop, right after leaving HDR | dividebysandwich/niri | none | [0a94298b](https://github.com/gonzalezerik/niri/commit/0a94298b), PR [#4](https://github.com/dividebysandwich/niri/pull/4) |
| The 10-bit probe frame's buffers were freed while the GPU could still be using them | dividebysandwich/niri | none | [724d3eb0](https://github.com/gonzalezerik/niri/commit/724d3eb0), PR [#5](https://github.com/dividebysandwich/niri/pull/5) |
| HDR → SDR → HDR (three mode switches) every time mpv goes fullscreen | dividebysandwich/niri | none | [08691922](https://github.com/gonzalezerik/niri/commit/08691922), PR [#6](https://github.com/dividebysandwich/niri/pull/6) |
| Games in nested gamescope start in SDR | dividebysandwich/niri | none | [bc31fd6d](https://github.com/gonzalezerik/niri/commit/bc31fd6d), PR [#7](https://github.com/dividebysandwich/niri/pull/7) |
| A game in nested gamescope stays SDR for the whole session if another window had focus when it created its swapchain | this branch's HDR (HDR is only offered to the focused fullscreen window) together with gamescope | none | Not fixed in niri. Workaround: `--hdr-debug-force-support` in the game's gamescope launch option |
| Nested gamescope loses its `app_id` after its window is unmapped, so the `^gamescope$` VRR window rule stops matching | gamescope | [ValveSoftware/gamescope#2446](https://github.com/ValveSoftware/gamescope/issues/2446) (closed) | Fixed upstream in [e0602ccc](https://github.com/ValveSoftware/gamescope/commit/e0602ccc313b6dee0dabb09c67e50a28eb784de9), not in a release yet (3.16.31); my NixOS config applies that commit to gamescope |
| Spider-Man Remastered crashes gamescope: gamescope passes on the game's invalid HDR metadata and niri rejects it with a protocol error | gamescope | none for this case; related: [ValveSoftware/gamescope#2249](https://github.com/ValveSoftware/gamescope/issues/2249) | niri is right to reject it. Workaround: the game's peak-brightness setting raised to the display's real peak |
| Firefox is disconnected mid-video: its `wp_viewport` source rectangle overshoots the buffer by 1/256 px | Firefox | [Mozilla bug 2062135](https://bugzilla.mozilla.org/show_bug.cgi?id=2062135) (assigned, patch in review) | niri follows the protocol here, so nothing to fix in niri. Firefox is no longer installed on either machine |
| Jellium Desktop's Wayland proxy deletes Flatpak's link to the compositor socket, so every later launch fails with "Connection refused" | Jellium Desktop | none | Flatpak override `--filesystem=xdg-run/wayland-1.lock:ro` |

### Virtual outputs

Bugs found while building virtual outputs on [niri-wm/niri#3800](https://github.com/niri-wm/niri/pull/3800). That work is on a separate local branch and is not in `erikg-hdr` yet; the commit ids below are on that branch.

| Bug | In | Upstream issue | Fix |
|---|---|---|---|
| Clients on a virtual output never get a frame reported as presented, so frame pacing falls back to guesswork | #3800 | none | `1b8ef36a` |
| #3800's redraw change also delays physical monitors by up to one refresh | #3800 | none | `1b8ef36a` (the change now applies to virtual outputs only) |
| `power-off-monitors` also stops virtual outputs, so a remote stream drops to about 1 frame per second; locking while monitors are off counted the session as locked without drawing the lock screen on them | #3800 | none; related, for physical monitors: [niri-wm/niri#3295](https://github.com/niri-wm/niri/issues/3295) (open) | `981ad038` |
| The headless backend renders on whichever GPU EGL picks, possibly software rendering, so capture clients like Sunshine lose zero-copy | #3800 | none | `e14d229a` (renders on `debug { render-drm-device }`) |
| A refresh rate of 0 or an extreme value is accepted (0 divides by zero) | #3800 | none; flagged in review on [niri-wm/niri#3953](https://github.com/niri-wm/niri/pull/3953) | `defe8a89` |
| On a machine with Mesa and the NVIDIA driver, the render node could be opened through the wrong EGL vendor and fall back to an arbitrary GPU | my own `e14d229a` | none | `0b20ed59` |

### No bugs found so far

Auto-rotate, per-output color profiles and the `float-above-fullscreen` window rule.

## Status

Personal and experimental; tested only on my two machines. Please don't report problems with this branch to upstream niri.

---

*The original niri README follows.*

<h1 align="center"><img alt="niri" src="https://github.com/user-attachments/assets/07d05cd0-d5dc-4a28-9a35-51bae8f119a0"></h1>
<p align="center">A scrollable-tiling Wayland compositor.</p>
<p align="center">
    <a href="https://matrix.to/#/#niri:matrix.org"><img alt="Matrix" src="https://img.shields.io/badge/matrix-%23niri-blue?logo=matrix"></a>
    <a href="https://github.com/niri-wm/niri/blob/main/LICENSE"><img alt="GitHub License" src="https://img.shields.io/github/license/niri-wm/niri"></a>
    <a href="https://github.com/niri-wm/niri/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/niri-wm/niri?logo=github"></a>
</p>

<p align="center">
    <a href="https://niri-wm.github.io/niri/Getting-Started.html">Getting Started</a> | <a href="https://niri-wm.github.io/niri/Configuration%3A-Introduction.html">Configuration</a> | <a href="https://github.com/niri-wm/niri/discussions/325">Setup&nbsp;Showcase</a>
</p>

<img width="1280" height="720" alt="niri with a few windows open" src="https://github.com/user-attachments/assets/dea5909e-1859-4aaa-9d88-d37f9663e00b" />

## About

Windows are arranged in columns on an infinite strip going to the right.
Opening a new window never causes existing windows to resize.

Every monitor has its own separate window strip.
Windows can never "overflow" onto an adjacent monitor.

Workspaces are dynamic and arranged vertically.
Every monitor has an independent set of workspaces, and there's always one empty workspace present all the way down.

The workspace arrangement is preserved across disconnecting and connecting monitors where it makes sense.
When a monitor disconnects, its workspaces will move to another monitor, but upon reconnection they will move back to the original monitor.

## Features

- Built from the ground up for scrollable tiling
- [Dynamic workspaces](https://niri-wm.github.io/niri/Workspaces.html) like in GNOME
- An [Overview](https://github.com/user-attachments/assets/379a5d1f-acdb-4c11-b36c-e85fd91f0995) that zooms out workspaces and windows
- Built-in screenshot UI
- Monitor and window screencasting through xdg-desktop-portal-gnome
    - You can [block out](https://niri-wm.github.io/niri/Configuration%3A-Window-Rules.html#block-out-from) sensitive windows from screencasts
    - [Dynamic cast target](https://niri-wm.github.io/niri/Screencasting.html#dynamic-screencast-target) that can change what it shows on the go
- [Touchpad](https://github.com/niri-wm/niri/assets/1794388/946a910e-9bec-4cd1-a923-4a9421707515) and [mouse](https://github.com/niri-wm/niri/assets/1794388/8464e65d-4bf2-44fa-8c8e-5883355bd000) gestures
- Group windows into [tabs](https://niri-wm.github.io/niri/Tabs.html)
- Configurable layout: gaps, borders, struts, window sizes
- [Gradient borders](https://niri-wm.github.io/niri/Configuration%3A-Layout.html#gradients) with Oklab and Oklch support
- [Background blur](https://niri-wm.github.io/niri/Window-Effects.html) for windows and layer-shell surfaces
- [Animations](https://github.com/niri-wm/niri/assets/1794388/ce178da2-af9e-4c51-876f-8709c241d95e) with support for [custom shaders](https://github.com/niri-wm/niri/assets/1794388/27a238d6-0a22-4692-b794-30dc7a626fad)
- Live-reloading config
- Works with [screen readers](https://niri-wm.github.io/niri/Accessibility.html)

## Video Demo

https://github.com/niri-wm/niri/assets/1794388/bce834b0-f205-434e-a027-b373495f9729

Also check out these videos that showcase a lot of the niri functionality:

- [Niri Is My New Favorite Wayland Compositor](https://www.youtube.com/watch?v=DeYx2exm04M) by Brodie Robertson
- [How Is niri This Good? Live Demo + Config](https://www.youtube.com/watch?v=7XmD5UyyhZQ) by Nick Janetakis

## Status

Niri is stable for day-to-day use and does most things expected of a Wayland compositor.
Many people are daily-driving niri, and are happy to help in our [Matrix channel].

Give it a try!
Follow the instructions on the [Getting Started](https://niri-wm.github.io/niri/Getting-Started.html) page.
Grab a desktop shell like [DankMaterialShell] or [Noctalia] (or build a more traditional setup): niri by itself is not a complete desktop environment.
Also check out [awesome-niri], a list of niri-related links and projects.

Here are some points you may have questions about:

- **Multi-monitor**: yes, a core part of the design from the very start. Mixed DPI works.
- **Fractional scaling**: yes, plus all niri UI stays pixel-perfect.
- **NVIDIA**: seems to work fine.
- **Floating windows**: yes, starting from niri 25.01.
- **Input devices**: niri supports tablets, touchpads, and touchscreens.
You can map the tablet to a specific monitor, or use [OpenTabletDriver].
We have touchpad gestures, but no touchscreen gestures yet.
- **Wlr protocols**: yes, we have most of the important ones like layer-shell, gamma-control, screencopy.
You can check on [wayland.app](https://wayland.app) at the bottom of each protocol's page.
- **Performance**: while I run niri on beefy machines, I try to stay conscious of performance.
I've seen someone use it fine on an Eee PC 900 from 2008, of all things.
- **Xwayland**: [integrated](https://niri-wm.github.io/niri/Xwayland.html#using-xwayland-satellite) via xwayland-satellite starting from niri 25.08.

## Media

[niri: Making a Wayland compositor in Rust](https://youtu.be/Kmz8ODolnDg?list=PLRdS-n5seLRqrmWDQY4KDqtRMfIwU0U3T) · *December 2024*

My talk from the 2024 Moscow RustCon about niri, and how I do randomized property testing and profiling, and measure input latency.
The talk is in Russian, but I prepared full English subtitles that you can find in YouTube's subtitle language selector.

[An interview with Ivan, the developer behind Niri](https://www.trommelspeicher.de/podcast/special_the_developer_behind_niri) · *June 2025*

An interview by a German tech podcast Das Triumvirat (in English).
We talk about niri development and history, and my experience building and maintaining niri.

[A tour of the niri scrolling-tiling Wayland compositor](https://lwn.net/Articles/1025866/) · *July 2025*

An LWN article with a nice overview and introduction to niri.

[How to test a Wayland compositor?](https://youtu.be/w44fYK5z418?list=PLK4NE4dSsLlw) · *October 2026*

My talk about some testing techniques used in niri: property-based tests, snapshot tests, client-server tests.
The first half overlaps with my RustCon talk, the second half is new.
The talk is in Russian, but I prepared full English subtitles that you can find in YouTube's subtitle language selector.

## Contributing

If you'd like to help with niri, there are plenty of both coding- and non-coding-related ways to do so.
See [CONTRIBUTING.md](https://github.com/niri-wm/niri/blob/main/CONTRIBUTING.md) for an overview.

## Inspiration

Niri is heavily inspired by [PaperWM] which implements scrollable tiling on top of GNOME Shell.

One of the reasons that prompted me to try writing my own compositor is being able to properly separate the monitors.
Being a GNOME Shell extension, PaperWM has to work against Shell's global window coordinate space to prevent windows from overflowing.

## Tile Scrollably Elsewhere

Here are some other projects which implement a similar workflow:

- [PaperWM]: scrollable tiling on top of GNOME Shell.
- [karousel]: scrollable tiling on top of KDE.
- [scroll](https://github.com/dawsers/scroll) and [papersway]: scrollable tiling on top of sway/i3.
- Hyprland has a built-in [scrolling layout](https://wiki.hypr.land/Configuring/Layouts/Scrolling-Layout/).
- [Paneru] and [PaperWM.spoon]: scrollable tiling on top of macOS.

## Contact

Our main communication channel is a Matrix chat, feel free to join and ask a question: https://matrix.to/#/#niri:matrix.org

We also have a community Discord server: https://discord.gg/vT8Sfjy7sx

[PaperWM]: https://github.com/paperwm/PaperWM
[waybar]: https://github.com/Alexays/Waybar
[fuzzel]: https://codeberg.org/dnkl/fuzzel
[awesome-niri]: https://github.com/niri-wm/awesome-niri
[karousel]: https://github.com/peterfajdiga/karousel
[papersway]: https://spwhitton.name/tech/code/papersway/
[Paneru]: https://github.com/karinushka/paneru
[PaperWM.spoon]: https://github.com/mogenson/PaperWM.spoon
[Matrix channel]: https://matrix.to/#/#niri:matrix.org
[OpenTabletDriver]: https://opentabletdriver.net/
[DankMaterialShell]: https://danklinux.com/
[Noctalia]: https://noctalia.dev/
