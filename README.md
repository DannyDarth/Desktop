# OpenMouse Desktop

A native desktop app for configuring gaming mice, built with
[GPUI](https://www.gpui.rs/). It talks to mice through the drivers in
[`@openmouse/protocol`](https://github.com/OpenMouse-Project/mouse-protocol),
which run inside an embedded QuickJS runtime on top of a native HID layer.

## Layout

- `gpui/` — the app. `gpui/js/` is the TypeScript bundled into the app
  (control layouts, device scanning, game profiles); `gpui/installer/` builds
  the Windows installer.
- `crates/openmouse-hid` — native HID transport. `crates/openmouse-presence` —
  Discord Rich Presence.
- `public/` — `games.json` (served from the CDN) and device artwork.

## Building

Needs Rust, Node.js and the repo's npm dependencies.

```sh
npm ci                 # esbuild + the protocol drivers (used by gpui/build.rs)
cd gpui
cargo run              # debug build
cargo test
cargo build --release
```

`OPENMOUSE_MOCK=slx` (or `ulx`, `x3`, comma separated; a trailing `!` hides
real devices) lists fake devices for looking at their UI without hardware.

Releases are cut by pushing a `v*.*.*` tag; see `.github/workflows/release.yml`.

## Windows macros (custom fork)

The native app has a **Macros** page. No browser or separate Bridge is required.
Macros are software sequences, saved in `%APPDATA%\openmouse\macros.json`;
this feature does not write firmware or onboard memory. Automatic triggers start
switched off. Closing the window to the tray keeps the app and macros running;
quitting the app stops them.

1. Open a game on the **Games** page, then choose **Macros** to edit its set.
   Installed, running and already-configured games also appear on the Macros page.
2. Choose **Add macro**, select it, then choose **Record input**. Recording starts
   after one second and captures keyboard presses/releases and mouse button clicks.
   Press **F8** to save; **Escape** cancels and keeps the previous saved sequence.
3. Choose a Ctrl+Alt+F1–F12 shortcut (F8 is reserved), or Mouse Back/Forward.
   Adjust delays and the repeat count. Changes save automatically.
4. Turn on **Enable automatic triggers**. OpenMouse detects the game's process,
   activates only its macro set and displays it in the editor. Closing that game
   restores another running game's set, or Default if no game runs. A game with
   no configured macros activates an empty set, rather than using another game's
   shortcuts. The same shortcut may be assigned separately in every game.

Detection uses the existing game catalog and polls about every two seconds while
OpenMouse is active, or ten seconds in the background. A newly detected game takes
priority when multiple games run. Games first detected in the same scan use a
stable catalog-order tie break. Alt-tabbing does not change the selected game.
Switching games stops playback and cancels an unfinished recording.

**Escape** stops playback; the page also has **Stop / cancel** and an enable
switch. Playback
releases every input it pressed on cancellation or error. One sequence plays at
a time; triggers during playback are ignored. A sequence has at most 1,000 events,
100 repeats, 60 seconds per delay and ten minutes total playback time.

This first version uses ordinary Windows keyboard and mouse input, so a G915 X
can trigger keyboard shortcuts and a G903 can trigger standard Back/Forward
buttons without relying on onboard Logitech macro support. Mouse triggers retain
their normal Back/Forward action. Dedicated G-keys, additional vendor-specific
buttons, wheel/cursor movement, remapping/suppressing triggers, custom game
executables and hardware-specific validation are not included yet. Windows can
reject input into an elevated application; the app reports rejected playback.

Engine checks without the GUI toolchain:

```sh
cargo test --manifest-path gpui/tests/macro-core/Cargo.toml --locked
```

## Portable Windows testing

The Windows build workflow produces **openmouse-windows-portable**. Download the
artifact, extract it, and run `OpenMouseDesktop.exe`. No installation is required.
Use the tray menu's Quit command before replacing builds or launching a different
copy; closing the window leaves it running.

Portable testing shares `%APPDATA%\openmouse` with the installed app, so settings,
macros and game profiles remain available when switching builds or moving to the
installer under the same Windows account. This is an install-free executable,
not a roaming settings bundle. The ZIP includes a README and source commit ID.

Pull-request builds now produce the portable artifact by default. To package an
installer from the same code, manually run **Build (Windows)** on the desired
branch with **Also build the Windows installer** enabled. Tagged release
packaging remains available through the existing release workflow.
