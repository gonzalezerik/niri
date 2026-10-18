# Rebasing onto a new niri release

`erikg-next` is a linear stack on upstream `main`; move it by rebasing,
never merging. `rerere` is enabled — keep `.git/rr-cache` so known
resolutions replay automatically.

1. Fetch and tag a backup:

   ```sh
   git fetch upstream
   git tag backup/erikg-next/$(date +%Y%m%d-%H%M)-rebase
   ```

2. Rebase onto the new base:

   ```sh
   git rebase --onto upstream/main $(git merge-base upstream/main erikg-next) erikg-next
   ```

   (The merge-base is the current base; compute it before rebasing.)

3. Resolve conflicts topic by topic, in branch order. Hot spots, hottest
   first:

   - `src/backend/tty.rs` — output color state, EDID parsing, color-profile
     application, the virtual-output connect and resize paths. The
     most-changed file on this branch; expect conflicts in every rebase.
   - `src/niri.rs` — HDR color-management wiring and blend space, the
     auto-rotate field/setter, color-profile state, virtual-output state
     and presentation-feedback wiring.
   - `src/tests/client.rs` — the test client harness: `State` fields, the
     registry bind chain and the `Dispatch` impls are shared by the HDR
     color-management, presentation-feedback and capture-constraint
     topics; expect conflicts in the import block and the registry
     handler.
   - `src/handlers/mod.rs` (`impl ColorManagementHandler for State`) and
     the `ColorManagementState::new` call in `src/niri.rs` — HDR protocol
     wiring.
   - `niri-config/src/lib.rs` — color-profile config options.
   - `src/layout/floating.rs` and `src/layout/workspace.rs` — the
     float-above-fullscreen render and hit-test threading; expect
     conflicts whenever upstream reworks the render pass.
   - `src/render_helpers/blend.rs`, `src/render_helpers/mod.rs`,
     `src/render_helpers/shaders/`, `src/screencasting/` and
     `src/niri.rs` — the HDR→SDR capture commit.
   - `src/render_helpers/mod.rs` and the capture blend threading in
     `src/niri.rs` — dmabuf captures follow the output blend space.

   A fix that belongs to an earlier commit goes in via
   `git commit --fixup=<sha>` + `git rebase -i --autosquash`, never as a
   new catch-all commit.

4. Gate every commit (or at least the tip; per-commit for release moves):

   ```sh
   cargo fmt --all -- --check
   cargo clippy --all --all-targets -- -D warnings
   cargo test --all --exclude niri-visual-tests
   cargo check --no-default-features
   cargo check --no-default-features --features dbus
   cargo check --no-default-features --features systemd
   cargo check --no-default-features --features dinit
   cargo check --no-default-features --features xdp-gnome-screencast
   cargo check --features auto-rotate
   cargo build -p niri-visual-tests
   typos
   ```

5. Check the result:

   ```sh
   git range-diff <old-base>..<old-tip> <new-base>..erikg-next
   ```

   Every `!` pair should be an intentional adaptation; document them.

## Bumping the smithay pin

Edit the `rev` in `Cargo.toml` (`[workspace.dependencies.smithay]` and
`[workspace.dependencies.smithay-drm-extras]`), run `cargo check` to
refresh `Cargo.lock`, then fold both files into the pin commit:

```sh
git commit --fixup=<pin-commit> && git rebase -i --autosquash <pin-commit>^
```
