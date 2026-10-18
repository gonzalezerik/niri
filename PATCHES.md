# Patches on this fork

`erikg-next` is upstream niri plus a linear series of patches, in topic
order. Each topic lists its commits, origin, Upstream-Status, and when it
can be dropped. When upstream merges an equivalent, the topic is dropped
from this branch, never kept alongside.

The branch sits on upstream `main` at the commit shown by
`git merge-base upstream/main erikg-next`; see `REBASING.md` for moving to
a newer release.

## smithay pin

- Build against my smithay fork for HDR support

Pin to `gonzalezerik/smithay` rev `248f78ce` (see that fork's `PATCHES.md`
for its own topics).
Upstream-Status: Fork-only (HDR needs smithay work not yet upstream).
Drop when: smithay upstream merges the color-management, drm color-state
and gles transform work, and niri moves to an upstream smithay release.

## typos allowlist

- typos: allowlist HDR protocol term and pre-existing upstream words

Upstream-Status: Fork-only (build hygiene).
Drop when: the HDR topics drop (`cll` becomes unused) and upstream fixes
the two allowlisted upstream words.

## HDR

- feat: initial HDR support for fullscreen applications
- hdr: apply connector color state via smithay's atomic commit
- hdr: advertise HDR to clients via color-management feedback
- hdr: search the surface tree for HDR image descriptions
- hdr: composite SDR content into the blend space of always-HDR outputs
- hdr: resolve subsurfaces to their root for preferred descriptions
- Implement probe if 10 bit rendering is actually available
- Replace probe with a test render_frame
- fix: probe 10-bit formats independently
- Support extended target volume, Windows scRGB, and custom primaries
- Advertise Windows BT.2100 support and wine HDR-detection fixes
- Encode Windows scRGB content on SDR frames instead of passing it raw
- Advertise ext_linear and composite extended-linear content generally
- fix: convert HDR content for SDR captures

Origin: dividebysandwich (HDR core and 10-bit probe), Christopher Snowhill
(extended target volume, scRGB, BT.2100, ext_linear), kakxem (10-bit probe
fix, HDR→SDR capture conversion).
Upstream-Status: Pending.
Drop when: niri merges HDR support upstream.

## HDR fixes and per-output color profiles

- hdr: don't free the swapchain on blend-space changes
- hdr: wait for the 10-bit probe frame before discarding its buffers
- Add per-output color profiles (ICC, EDID) via the CRTC color pipeline
- hdr: ride out a momentary loss of the HDR description
- hdr: prefer PQ for a window as soon as it is told to go fullscreen
- hdr: add off/on/auto mode values, default off

Behavior change: an `hdr` node without `mode` used to mean auto (engage for
fullscreen HDR content); now a bare `hdr` node means off, and niri warns at
config load when an `hdr` node has no `mode`.

Origin: Erik Gonzalez.
Upstream-Status: Fork-only (stability fixes for the HDR topics; color
profiles drive the CRTC degamma/CTM/gamma pipeline, which upstream niri
does not manage).
Drop when: upstream provides equivalent HDR stability and per-output
color-profile management.

## auto-rotate

- Add auto-rotate (niri-wm/niri PR #3763)
- Make auto-rotate an opt-in cargo feature

Origin: Rio Sanjaya, <https://github.com/niri-wm/niri/pull/3763>; the
opt-in feature gate is my own.
Upstream-Status: Submitted <https://github.com/niri-wm/niri/pull/3763>.
Drop when: that PR merges upstream.

## float-above-fullscreen

- window-rule: Add `float-above-fullscreen`
- Docs for window-rule `float-above-fullscreen`

Fix: the config parse test's snapshot in the first commit was missing
`float_above_fullscreen: None`, so that commit failed `cargo test` on its
own; I added the missing line.

Origin: Zhai Can, <https://github.com/niri-wm/niri/pull/4062>.
Upstream-Status: Submitted <https://github.com/niri-wm/niri/pull/4062>.
Drop when: that PR merges upstream.

## virtual-outputs

- Add virtual output support
- virtual: share virtual output state and validate input
- virtual: add --mode/--scale to create-virtual-output, resize in place
- virtual: take presentation feedbacks, scope pacing changes to virtual outputs
- headless: render on the configured render node
- tests: expand virtual output coverage
- docs: rewrite Virtual-Outputs.md for the final CLI
- headless: try every EGL device matching the render node
- tests: add presentation-feedback client harness and pacing tests
- virtual: keep rendering while monitors are powered off

Origin: Willian Barreto, <https://github.com/niri-wm/niri/pull/3800>; the
follow-up work on top is my own.
Upstream-Status: Willian Barreto's commit is Submitted
<https://github.com/niri-wm/niri/pull/3800>; the rest is Fork-only.
Drop when: that PR merges upstream and covers the follow-up work.

## capture

- capture: Render dmabuf captures in the output's blend space
- tests: Cover the capture color contract

Origin: Erik Gonzalez.
Upstream-Status: Pending (only meaningful alongside the HDR topics).
Drop when: niri merges HDR support upstream with an equivalent capture
color contract.

## fork README

- README: describe the erikg-hdr branch

Upstream-Status: Fork-only (fork documentation).
Drop when: never.
