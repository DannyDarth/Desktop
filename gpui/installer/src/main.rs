// A release build is a GUI app: no console window behind it on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use gpui_kit::assets::IconName;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::gpui::ease_in_out;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, ActiveTheme as _, Icon, Sizable as _, Size, Theme, ThemeMode, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use openmouse_installer::paths;
use std::time::Duration;

/// Fade + slight rise, same feel as the main app's title-badge reveal
/// (`gpui/src/app.rs`'s `BADGE_FADE`) — just slower, since this is a whole
/// screen swapping rather than a small hover detail.
const STEP_TRANSITION: Duration = Duration::from_millis(260);
/// Breathing pulse on the in-progress step dot.
const DOT_PULSE: Duration = Duration::from_millis(900);

const LOGO_PNG: &[u8] = include_bytes!("../../assets/logo.png");

/// Same binary, two roles: run normally it's the installer; copied into the
/// install directory and invoked with `--uninstall` (how the Add/Remove
/// Programs entry's `UninstallString` calls it — see `registry.rs`) it's the
/// uninstaller. Falling back to a filename sniff covers someone just
/// double-clicking the copy left behind in the install folder.
fn uninstall_mode() -> bool {
    if std::env::args().any(|a| a == openmouse_installer::UNINSTALL_ARG) {
        return true;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .is_some_and(|name| name.contains("uninstall"))
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    /// Welcome (install) or confirm-removal (uninstall) — step 1.
    Start,
    /// Copying files / removing them — step 2.
    Working,
    /// Step 3. Carries whether it ended in an error.
    Done,
}

/// What the device scan on the welcome screen has found so far.
enum DeviceScan {
    Scanning,
    Found(Vec<openmouse_installer::devices::Device>),
    /// The scan could not run; the card says so rather than claiming none.
    Unavailable,
}

struct InstallerView {
    uninstall_mode: bool,
    screen: Screen,
    error: Option<String>,
    desktop_shortcut: bool,
    devices: DeviceScan,
}

impl InstallerView {
    fn new(uninstall_mode: bool) -> Self {
        Self { uninstall_mode, screen: Screen::Start, error: None, desktop_shortcut: true, devices: DeviceScan::Scanning }
    }

    /// Looks for plugged-in supported devices in the background and shows them
    /// on the welcome screen. Install does not wait on it.
    fn scan_devices(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            let result = cx.background_executor().spawn(async { openmouse_installer::devices::scan() }).await;
            let _ = view.update(cx, |view, cx| {
                view.devices = match result {
                    Ok(found) => DeviceScan::Found(found),
                    Err(_) => DeviceScan::Unavailable,
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The "your devices" card: the supported devices that are plugged in now.
    fn devices_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().primary;
        let border = cx.theme().border;
        const SHOWN: usize = 4;
        let heading = div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(muted).child("SUPPORTED DEVICES DETECTED");
        let body: AnyElement = match &self.devices {
            DeviceScan::Scanning => h_flex()
                .gap_2()
                .items_center()
                .child(Spinner::new().with_size(Size::Small).color(accent).into_any_element())
                .child(div().text_sm().text_color(muted).child("Looking for your mice…"))
                .into_any_element(),
            DeviceScan::Unavailable => {
                div().text_sm().text_color(muted).child("Couldn’t check for devices. OpenMouse will find them when it starts.").into_any_element()
            }
            DeviceScan::Found(found) if found.is_empty() => div()
                .text_sm()
                .text_color(muted)
                .child("None plugged in right now. OpenMouse picks a device up as soon as you plug one in.")
                .into_any_element(),
            DeviceScan::Found(found) => div()
                .flex()
                .flex_col()
                .gap_1p5()
                .children(found.iter().take(SHOWN).map(|device| {
                    let name = if device.name.trim().is_empty() { device.brand.clone() } else { device.name.clone() };
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(IconName::Check).size(px(14.)).text_color(accent))
                        .child(div().text_sm().text_color(fg).child(name))
                        .child(div().text_xs().text_color(muted).child(device.brand.clone()))
                }))
                .when(found.len() > SHOWN, |col| {
                    col.child(div().text_xs().text_color(muted).child(format!("and {} more", found.len() - SHOWN)))
                })
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .child(heading)
            .child(body)
    }

    fn step_index(&self) -> usize {
        match self.screen {
            Screen::Start => 0,
            Screen::Working => 1,
            Screen::Done => 2,
        }
    }

    fn start_install(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Working;
        cx.notify();
        let desktop_shortcut = self.desktop_shortcut;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { openmouse_installer::install(desktop_shortcut, |_| {}) })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.screen = Screen::Done;
                view.error = result.err();
                cx.notify();
            });
        })
        .detach();
    }

    fn start_uninstall(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Working;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx.background_executor().spawn(async move { openmouse_installer::uninstall() }).await;
            let _ = view.update(cx, |view, cx| {
                if result.is_ok() {
                    openmouse_installer::spawn_self_delete(&paths::install_dir());
                }
                view.screen = Screen::Done;
                view.error = result.err();
                cx.notify();
            });
        })
        .detach();
    }

    fn launch_app(&self) {
        let _ = std::process::Command::new(paths::app_exe_path()).spawn();
    }

    /// Three dots tracking progress through Start → Working → Done, filled
    /// green up to the current step — the "step by step" wizard framing. The
    /// in-progress dot breathes while `Working` is running; the rest are static.
    fn step_dots(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.step_index();
        let working = self.screen == Screen::Working;
        let accent = cx.theme().primary;
        let border = cx.theme().border;
        h_flex().gap_2().children((0..3).map(move |i| {
            let dot = div().size(px(6.)).rounded_full().bg(if i <= current { accent } else { border });
            if working && i == current {
                dot.with_animation(
                    ElementId::Integer(i as u64),
                    Animation::new(DOT_PULSE).repeat().with_easing(ease_in_out),
                    |this, delta| {
                        // Triangle wave: 1.0 → 0.4 → 1.0, not a one-shot fade.
                        let pulse = 1.0 - 0.6 * (1.0 - (2.0 * delta - 1.0).abs());
                        this.opacity(pulse)
                    },
                )
                .into_any_element()
            } else {
                dot.into_any_element()
            }
        }))
    }

    fn header(&self, subtitle: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let logo = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec()));
        h_flex()
            .gap_3()
            .items_center()
            .child(img(logo).w(px(28.)).h(px(28.)).object_fit(ObjectFit::Contain))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).text_color(fg).child("OpenMouse Desktop"))
                    .child(div().text_xs().text_color(muted).child(subtitle.to_string())),
            )
    }
}

impl Render for InstallerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().primary;
        let border = cx.theme().border;
        let surface = cx.theme().muted;

        let subtitle = if self.uninstall_mode { "Uninstall" } else { "Setup" };
        let header = self.header(subtitle, cx).into_any_element();
        let dots = self.step_dots(cx).into_any_element();

        let body: AnyElement = match self.screen {
            Screen::Start if self.uninstall_mode => div()
                .flex()
                .flex_col()
                .gap_5()
                .child(div().text_sm().text_color(muted).child(
                    "This removes OpenMouse Desktop, its shortcuts, and its Add/Remove Programs entry. Your games, macros, lighting preferences, and other saved settings are kept.",
                ))
                .child(
                    h_flex()
                        .gap_3()
                        .justify_end()
                        .child(Button::new("cancel").label("Cancel").on_click(cx.listener(|_, _, window, _| window.remove_window())))
                        .child(
                            Button::new("uninstall")
                                .primary()
                                .label("Uninstall")
                                .on_click(cx.listener(|this, _, _, cx| this.start_uninstall(cx))),
                        ),
                )
                .into_any_element(),
            Screen::Start => div()
                .flex()
                .flex_col()
                .gap_5()
                .child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(surface)
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(format!("Installs to {}", paths::install_dir().display())),
                )
                .child(self.devices_card(cx))
                .child(
                    Checkbox::new("desktop-shortcut")
                        .checked(self.desktop_shortcut)
                        .label("Create a desktop shortcut")
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.desktop_shortcut = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    div().flex().justify_end().child(
                        Button::new("install")
                            .primary()
                            .label("Install")
                            .on_click(cx.listener(|this, _, _, cx| this.start_install(cx))),
                    ),
                )
                .into_any_element(),
            Screen::Working => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Spinner::new().with_size(Size::Large).color(accent).into_any_element())
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(if self.uninstall_mode { "Removing…" } else { "Installing…" }),
                )
                .into_any_element(),
            Screen::Done => {
                let (headline, message): (&str, String) = match (&self.error, self.uninstall_mode) {
                    (Some(error), _) => ("Something went wrong", error.clone()),
                    (None, true) => ("All done", "OpenMouse Desktop has been removed.".to_string()),
                    (None, false) => ("You're all set", "OpenMouse Desktop is installed. Your saved games, macros, and settings are ready to use.".to_string()),
                };
                // Pops in with the same fade+rise as the step transition
                // below, just keyed separately so it re-triggers on its own.
                let badge = div()
                    .size(px(40.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if self.error.is_some() { cx.theme().danger } else { accent })
                    .child(Icon::new(if self.error.is_some() { IconName::X } else { IconName::Check }).size(px(20.)).text_color(cx.theme().background))
                    .with_animation(
                        ElementId::Name("done-badge".into()),
                        Animation::new(STEP_TRANSITION).with_easing(ease_out_cubic),
                        |this, delta| this.opacity(delta),
                    );
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(badge)
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).text_color(fg).child(headline))
                    .child(div().text_sm().text_color(muted).child(message))
                    .child(
                        h_flex()
                            .gap_3()
                            .justify_end()
                            .child(Button::new("close").label("Close").on_click(cx.listener(|_, _, window, _| window.remove_window())))
                            .when(self.error.is_none() && !self.uninstall_mode, |row| {
                                row.child(
                                    Button::new("launch")
                                        .primary()
                                        .label("Launch OpenMouse Desktop")
                                        .on_click(cx.listener(|this, _, window, _| {
                                            this.launch_app();
                                            window.remove_window();
                                        })),
                                )
                            }),
                    )
                    .into_any_element()
            }
        };

        // Fade + slight rise each time the step changes, keyed by step index
        // so moving forward (or back to an error view) always re-triggers it
        // instead of only playing once per view lifetime.
        let animated_body = div()
            .size_full()
            .child(body)
            .with_animation(
                ElementId::NamedInteger("installer-step".into(), self.step_index() as u64),
                Animation::new(STEP_TRANSITION).with_easing(ease_out_cubic),
                |this, delta| this.opacity(delta).top(px(6.0 * (1.0 - delta))),
            );

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(fg)
            .flex()
            .flex_col()
            .p_6()
            .gap_5()
            .child(header)
            .child(dots)
            .child(div().flex_1().child(animated_body))
    }
}

fn main() {
    let uninstall = uninstall_mode();
    application().with_assets(assets::AllAssets).run(move |cx| {
        init(cx);
        // No explicit `window_bounds` otherwise defaults to filling the
        // display — this is a small dialog, not a resizable app window.
        let window_size = size(px(480.), px(440.));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
            window_min_size: Some(window_size),
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..TitleBar::window_options()
        };
        let (handle, view) = open_window(options, cx, |_window, cx| cx.new(|_| InstallerView::new(uninstall)))
            .expect("failed to open installer window");
        // Applied after the window exists, not inside the build closure:
        // `Theme::change`/`Theme::update` mid-construction poisons first
        // render (same ordering the main app's `bin/main.rs` documents).
        let _ = cx.update_window(handle, |_, window, cx| {
            Theme::change(ThemeMode::Dark, Some(window), cx);
            openmouse_installer::colors::paint(cx);
        });
        if !uninstall {
            view.update(cx, |view, cx| view.scan_devices(cx));
        }
        cx.activate(true);
    });
}
