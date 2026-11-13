//! The on-screen game toast: a small always-on-top, click-through pill shown in
//! a corner of the screen the game is on, never under the menu bar, Dock or
//! taskbar. Used for profile apply/restore messages while a game has focus, so
//! it lives in its own window (the main window may be minimized or hidden).
//!
//! It must not take focus from the game: the window is opened without focus and
//! made click-through (see `native_window::configure_overlay`).

use std::time::Duration;

use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::*;

use crate::native_window::{configure_overlay, OverlayCorner};

/// Same pill as the Tauri overlay (`OVERLAY_SIZE`, `OVERLAY_MARGIN`).
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 58.0;
const MARGIN: f32 = 24.0;
const VISIBLE_FOR: Duration = Duration::from_secs(4);

pub struct ProfileNotice {
    pub title: String,
    pub subtitle: String,
    pub detail: String,
}
struct OverlayView {
    text: String,
    error: bool,
    profile: Option<ProfileNotice>,
}

impl Render for OverlayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(profile) = &self.profile {
            return div()
                .size_full()
                .flex()
                .items_center()
                .gap_4()
                .px_4()
                .py_4()
                .rounded_lg()
                .bg(cx.theme().sidebar)
                .child(
                    div()
                        .w(px(4.))
                        .h_full()
                        .flex_none()
                        .rounded_full()
                        .bg(cx.theme().primary),
                )
                .child(
                    Icon::new(gpui_kit::assets::IconName::Mouse)
                        .size(px(32.))
                        .text_color(cx.theme().primary),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(profile.title.clone()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(profile.subtitle.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(profile.detail.clone()),
                        ),
                )
                .into_any_element();
        }
        let accent = if self.error {
            cx.theme().danger
        } else {
            cx.theme().primary
        };
        div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .rounded_full()
            // No outline: a hairline border on a transparent window picks up a
            // dark fringe from edge antialiasing. The accent dot carries the colour.
            .bg(cx.theme().sidebar)
            .child(div().size(px(10.)).flex_none().rounded_full().bg(accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .truncate()
                    .child(self.text.clone()),
            )
            .into_any_element()
    }
}

/// Shows `text` in the overlay, replacing any message still up, and removes it
/// after a few seconds. Returns the window so the caller can replace it later.
pub fn show(
    cx: &mut App,
    previous: Option<AnyWindowHandle>,
    text: String,
    error: bool,
    corner: OverlayCorner,
) -> Option<AnyWindowHandle> {
    show_window(cx, previous, text, error, corner, None)
}
pub fn show_profile(
    cx: &mut App,
    previous: Option<AnyWindowHandle>,
    profile: ProfileNotice,
) -> Option<AnyWindowHandle> {
    show_window(
        cx,
        previous,
        String::new(),
        false,
        OverlayCorner::TopRight,
        Some(profile),
    )
}
fn show_window(
    cx: &mut App,
    previous: Option<AnyWindowHandle>,
    text: String,
    error: bool,
    corner: OverlayCorner,
    profile: Option<ProfileNotice>,
) -> Option<AnyWindowHandle> {
    let height = if profile.is_some() { 120.0 } else { HEIGHT };
    let visible_for = if profile.is_some() {
        Duration::from_secs(3)
    } else {
        VISIBLE_FOR
    };
    if let Some(old) = previous {
        let _ = old.update(cx, |_, window, _| window.remove_window());
    }

    // Off macOS the position comes from the display's visible bounds (which
    // exclude the taskbar); on macOS `configure_overlay` places it after opening.
    let origin = cx
        .primary_display()
        .map(|display| {
            let area = display.visible_bounds();
            let (w, h) = (px(WIDTH), px(height));
            let m = px(MARGIN);
            let x = match corner {
                OverlayCorner::TopLeft | OverlayCorner::BottomLeft => area.origin.x + m,
                _ => area.origin.x + area.size.width - w - m,
            };
            let y = match corner {
                OverlayCorner::TopLeft | OverlayCorner::TopRight => area.origin.y + m,
                _ => area.origin.y + area.size.height - h - m,
            };
            point(x, y)
        })
        .unwrap_or_else(|| point(px(MARGIN), px(MARGIN)));

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: size(px(WIDTH), px(height)),
        })),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let handle = cx
        .open_window(options, move |_, cx| {
            cx.new(|_| OverlayView {
                text,
                error,
                profile,
            })
        })
        .ok()?;
    configure_overlay(WIDTH as f64, height as f64, corner, MARGIN as f64);
    #[cfg(windows)]
    {
        // SetWindowPos/SetWindowLongPtr synchronously dispatch Win32 messages.
        // Never call them while the caller holds App/Context's RefCell borrow:
        // GPUI's window procedure re-enters AsyncApp::update_window.
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let hwnd = handle
                .update(cx, |_, window, _| {
                    match HasWindowHandle::window_handle(&*window).map(|h| h.as_raw()) {
                        Ok(RawWindowHandle::Win32(h)) => Some(h.hwnd.get()),
                        _ => None,
                    }
                })
                .ok()
                .flatten();
            // The update above has returned, so its UI borrow is released.
            // If a newer notification already removed this window, update
            // returns an error and no native call is made for its old HWND.
            if let Some(hwnd) = hwnd {
                crate::native_window::configure_overlay_hwnd(
                    hwnd,
                    WIDTH as f64,
                    height as f64,
                    corner,
                    MARGIN as f64,
                );
                // Flush refresh effects even while the main window is inactive.
                // This does not activate either the popup or the main window.
                cx.refresh();
            }
        })
        .detach();
    }

    let any: AnyWindowHandle = handle.into();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(visible_for).await;
        let _ = any.update(cx, |_, window, _| window.remove_window());
    })
    .detach();
    Some(any)
}
