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
3. Choose **Assign input**, wait one second, then press a keyboard key or shortcut,
   or click a mouse button. Left, right, middle, Back and Forward clicks are
   supported. Modifier-only bindings are captured when the modifier is released.
   Escape cancels assignment and stays reserved for stopping playback. F8 can
   activate a macro outside recording; during recording it remains the save key.
   The activation input must be released before it can play the macro.
4. After saving a recording, drag each event row onto a green insertion line to
   change the order. Drop above the first row or below the last row to move to
   either end. Each event keeps its delay. Incomplete press/release orders can
   be saved and reopened, but cannot play until corrected. Use the compact minus
   and plus controls to adjust delays, and set the repeat count as needed.
   Changes save automatically.
5. Turn on **Enable automatic triggers**. OpenMouse detects the game's process,
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
100 counted repeats, 60 seconds per delay and ten minutes per configured pass
(or per complete counted playback). Hold and toggle types continue until their
activation input or Stop/Escape ends them.

This first version uses ordinary Windows keyboard and mouse input, so a G915 X
can trigger keys/shortcuts and a G903 can trigger standard Windows mouse
buttons without relying on onboard Logitech macro support. Triggers retain
their normal key/click action. Dedicated G-keys, additional vendor-specific
buttons, wheel/cursor movement, remapping/suppressing triggers, hardware-specific validation are not included yet. Windows can
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


## Macro types

Choose a type in the macro editor:

| Type | Activation behavior |
| --- | --- |
| No repeat | Plays the recording once per activation press. |
| Repeat while holding | Repeats until the activation input is released. |
| Toggle | Repeats until the same activation input is pressed again. |
| Sequence | Runs On press once, repeats While held, then runs On release once. |

For Sequence, select **On press**, **While held** or **On release** before
recording or dragging events. Each section saves separately. Playback validates
that repeated sections return to their starting held-key state and that On
release releases keys held by On press. A section can be empty.

**Play once** previews one pass of a macro (all three sections for Sequence).
To test repeat/hold/toggle behavior, enable automatic triggers and activate the
macro in another app, such as Notepad. Automatic triggers are suspended while
OpenMouse itself is the foreground window, so bindings such as Left click do
not fire while editing the app.

Old macros without a type retain their saved repeat counts until you choose one
of the four types. Existing recordings and bindings are preserved. Physical
keyboard/mouse monitoring ignores injected output, preventing a macro from
stopping or retriggering itself. Escape, Stop, and game/profile changes cancel
playback and release injected held inputs.


## Focused game macro profiles (Windows)

Select a game's set on the Macros page and choose **While focused** under
**Activate game macros**. That set is active only while the game's executable
owns the foreground window. Alt+Tab immediately requests cancellation of any
playing macro, releases injected held inputs, and selects Default or another
eligible While running profile. Returning to the game reactivates its set;
previously stopped playback does not resume automatically.

Foreground checks run about every 100 ms, independently of the slower process
scan. This option is saved per game. **While running** remains the default for
existing profiles and retains their behavior when alt-tabbing. This controls
macro sets; existing device DPI/onboard game profiles keep their existing
process-based switching. Game detection supports both the bundled catalog and locally added executable paths.

## Editing names and exact delays

Choose **Rename** beside the selected macro's title to edit its name. Names are
trimmed and must contain 1 to 100 characters. Click an event's **ms** value to
enter an exact whole-number delay from 0 to 60000 milliseconds. The minus/plus
buttons remain available for 10 ms adjustments.

Press Enter or click away to save either edit. Escape cancels it. Invalid input
shows an inline error and does not replace the saved value. Typing in a delay
field does not start a row drag. Delay edits apply to the selected Sequence
section when using Sequence mode.


## Add any game executable

Choose **Add game** on Games or Macros, then select the game's actual `.exe`.
OpenMouse saves a personal entry in `%APPDATA%\openmouse\custom-games.json`
and opens its macro set immediately. This does not launch the executable or
require a new app build. Select **While running** or **While focused** for that
set as usual. The initial display name comes from the executable filename.

Custom entries match the selected executable's full path, so an unrelated app
with the same filename does not activate the profile. Adding the same path
again reopens its existing set instead of creating a duplicate. Entries survive
restarts and future portable/installer updates; they remain separate from the
bundled catalog. If a game is moved to another folder, add its new executable
path. Choose the game's executable rather than Steam, another launcher, or a
`.lnk` shortcut. Personal entries use their executable icon, with a name-initial placeholder if extraction is unavailable. Online cover artwork lookup is deferred.

## Macro profile notifications

A medium notification appears in the upper-right corner when the active macro
set changes. It shows the game's name and While focused/While running, or
Desktop and the Default profile when switching back. It disappears after three
seconds, is click-through, and opens without taking focus. New messages replace
old ones rather than stacking. Unchanged polling results do not repeat it.

When automatic triggers are off, the message says profile selected and notes
that triggers are off. The existing game-notification switch in Settings
controls these popups too. Profile notices are silent. Native desktop overlays
may not appear above a game's exclusive-fullscreen presentation; borderless
windowed mode allows normal desktop overlays.


## Executable profile images (Windows)

Added games use the icon associated with their `.exe` on Games and in the macro
profile picker. Existing personal entries get icons on startup too. Extraction
runs in the background without launching the game; cached PNGs avoid repeating
it on every visit. Updating the executable changes the icon cache key. Icons
are shown contained rather than stretched into cover artwork. If a file has no
readable icon, its existing name-initial placeholder remains available.

### Profile artwork and Logitech keyboards
Added games automatically match their Steam installation manifest and download a cover. In a game's Macros or device profile view, **Choose image** accepts PNG, JPEG or BMP (up to 20 MB); **Find Steam cover** retries the lookup and replaces a manual image only after a successful download. Manual artwork is copied into `%APPDATA%\openmouse\profile-images`, so moving the original image or updating the application does not remove it. Non-Steam games can use a chosen image or the executable icon fallback.

The native app now includes the Logitech keyboard protocol driver. Direct G915 X devices use this driver instead of a mouse driver; Logitech receivers retain mouse detection and can also identify a paired keyboard. Keyboard lighting controls include advertised brightness/backlight, shared RGB effects and static color presets (modern RGB zones or verified legacy fixed-color zones). Unsupported controls stay hidden. Lighting changes are live; onboard persistence is not guaranteed. Connected keyboards display an offline keyboard illustration, with a compact layout for TKL models. Macro recording and assignment use Windows input hooks independently of the HID connection. G915 X hardware verification is pending.

On startup and Rescan, detected devices connect automatically one at a time. A failed or sleeping device does not block later devices after its probe finishes. Rescan retries failed devices; automatic connections preserve the selected device.

Keyboard refinements: Lighting now has its own tab; Advanced shows detected model, firmware, connection and driver support notes. The G915-family effect adapter uses the keyboard software-control handshake documented in [OpenRGB's G915 controller](https://github.com/CalcProgrammer1/OpenRGB/blob/master/Controllers/LogitechController/LogitechG915Controller/LogitechG915Controller.cpp), propagates failures and supplies nonzero color for breathing. Cycling and wave require hardware confirmation. G915 X wired, LIGHTSPEED and TKL cards use their corresponding official Logitech product photos, cached locally; other models retain the offline illustration. Product matching cannot identify chassis color or regional key legends, so photos use the default black US variant.

G915 X Cycling now uses app-generated per-key color frames when 0x8081 zones are available, using the working solid-color path rather than the unconfirmed onboard Cycling effect. OpenMouse must remain running. Frames execute sequentially and close the HID handle between frames; choosing a solid color, another effect, or backlight Off stops the loop. Disconnects or failed acknowledgements stop animation and surface an error rather than retrying indefinitely. Brightness is independent of the loop. Hardware verification remains pending. Advanced information uses vertically stacked labels and wrapped text.
