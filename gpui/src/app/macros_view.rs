use super::*;
use crate::macros::{Macro, Trigger};

impl AppView {
    pub(super) fn render_macros(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.macro_engine.snapshot();
        let scope = self.macro_scope.clone();
        let scope_name = scope
            .as_deref()
            .map(|id| self.game_name(id))
            .unwrap_or_else(|| "Default".into());
        let active_name = state
            .active_game
            .as_deref()
            .map(|id| self.game_name(id))
            .unwrap_or_else(|| "Default".into());
        let busy = state.playing || state.recording.is_some();
        let selected = self.macro_selected.filter(|i| {
            state
                .library
                .macros
                .get(*i)
                .is_some_and(|m| m.game_id == scope)
        });
        let mut scopes = h_flex().gap_2().flex_wrap().child(self.chip(
            "macro-default".into(),
            "Default".into(),
            scope.is_none(),
            cx,
            |this, cx| {
                this.macro_scope = None;
                this.macro_selected = None;
                cx.notify();
            },
        ));
        for game in self.games.iter().filter(|g| {
            g.installed
                || g.running
                || scope.as_deref() == Some(&g.id)
                || state
                    .library
                    .macros
                    .iter()
                    .any(|m| m.game_id.as_deref() == Some(&g.id))
        }) {
            let id = game.id.clone();
            scopes = scopes.child(self.chip(
                format!("macro-game-{id}"),
                game.name.clone(),
                scope.as_deref() == Some(&id),
                cx,
                move |this, cx| {
                    this.macro_scope = Some(id.clone());
                    this.macro_selected = None;
                    cx.notify();
                },
            ));
        }
        let mut list = div().flex().flex_col().gap_2();
        for (i, m) in state
            .library
            .macros
            .iter()
            .enumerate()
            .filter(|(_, m)| m.game_id == scope)
        {
            list = list.child(self.chip(
                format!("macro-{i}"),
                format!(
                    "{} · {} · {} steps",
                    m.name,
                    m.trigger.label(),
                    if state.recording == Some(i) {
                        state.recorded_steps.len()
                    } else {
                        m.steps.len()
                    }
                ),
                selected == Some(i),
                cx,
                move |this, cx| {
                    this.macro_selected = Some(i);
                    cx.notify();
                },
            ));
        }
        if !state.library.macros.iter().any(|m| m.game_id == scope) {
            list = list.child(div().text_sm().child("No macros in this set. Add a macro, then record its keyboard presses and mouse clicks."));
        }
        let mut editor = div().flex().flex_col().gap_3();
        if let Some(i) = selected {
            let m = &state.library.macros[i];
            editor = editor
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(m.name.clone()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            Button::new("macro-record")
                                .label(if state.recording == Some(i) {
                                    "F8 saves recording"
                                } else {
                                    "Record input"
                                })
                                .disabled(
                                    !cfg!(windows) || state.playing || state.recording.is_some(),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.macro_engine.record(i);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("macro-play")
                                .label("Play")
                                .disabled(
                                    !cfg!(windows)
                                        || busy
                                        || m.steps.is_empty()
                                        || scope != state.active_game,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.macro_engine.play(i);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("macro-delete")
                                .label("Delete macro")
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.macro_engine.edit(|l| {
                                        l.macros.remove(i);
                                    });
                                    this.macro_selected = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(div().text_sm().child("Trigger (click to change):"));
            let mut trigger_row = h_flex().gap_2().flex_wrap();
            let triggers = (0x70..=0x7b)
                .filter(|k| *k != 0x77)
                .map(|key| Trigger::Shortcut { key })
                .chain([Trigger::Mouse { button: 4 }, Trigger::Mouse { button: 5 }]);
            for (n, trigger) in triggers.enumerate() {
                let assigned =
                    state.library.macros.iter().enumerate().any(|(j, other)| {
                        i != j && other.game_id == scope && other.trigger == trigger
                    });
                if assigned || busy {
                    continue;
                }
                trigger_row = trigger_row.child(self.chip(
                    format!("macro-trigger-{n}"),
                    trigger.label(),
                    m.trigger == trigger,
                    cx,
                    move |this, cx| {
                        this.macro_engine
                            .edit(|l| l.macros[i].trigger = trigger.clone());
                        cx.notify();
                    },
                ));
            }
            editor = editor.child(trigger_row).child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("macro-repeat-less")
                            .label("−")
                            .disabled(busy || m.repeats <= 1)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.macro_engine.edit(|l| {
                                    l.macros[i].repeats =
                                        l.macros[i].repeats.saturating_sub(1).max(1)
                                });
                                cx.notify();
                            })),
                    )
                    .child(format!("Repeat {} times", m.repeats))
                    .child(
                        Button::new("macro-repeat-more")
                            .label("+")
                            .disabled(busy || m.repeats >= 100)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.macro_engine.edit(|l| {
                                    l.macros[i].repeats = (l.macros[i].repeats + 1).min(100)
                                });
                                cx.notify();
                            })),
                    ),
            );
            if state.recording == Some(i) {
                let mut live = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .border_1()
                    .border_color(cx.theme().primary)
                    .rounded(cx.theme().radius)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "Live input · {} events recorded",
                                state.recorded_steps.len()
                            )),
                    );
                if state.recorded_steps.is_empty() {
                    live = live.child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("After the countdown, press a key or click a mouse button. F8 saves; Escape cancels."));
                } else {
                    live = live.child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child("Latest events first (up to 12 shown). All captured events are saved with F8."));
                    for (j, step) in state.recorded_steps.iter().enumerate().rev().take(12) {
                        live = live.child(
                            h_flex()
                                .gap_3()
                                .items_center()
                                .flex_wrap()
                                .child(div().min_w(px(220.)).text_sm().child(format!(
                                    "{}. {}",
                                    j + 1,
                                    crate::macros::step_label(step)
                                )))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{} ms", step.delay_ms)),
                                ),
                        );
                    }
                }
                editor = editor.child(live);
            } else {
                editor = editor.child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child("Reorder presses and releases separately. Delays move with their events. A release must stay after its matching press."));
                for (j, step) in m.steps.iter().enumerate() {
                    let last = m.steps.len() - 1;
                    let mut moves = h_flex().gap_1().flex_wrap();
                    for (action, label, target) in [
                        ("first", "To start", 0),
                        ("up", "Up", j.saturating_sub(1)),
                        ("down", "Down", (j + 1).min(last)),
                        ("last", "To end", last),
                    ] {
                        moves = moves.child(
                            Button::new(SharedString::from(format!("macro-move-{action}-{j}")))
                                .compact()
                                .label(label)
                                .disabled(busy || target == j)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.macro_engine.move_step(i, j, target);
                                    cx.notify();
                                })),
                        );
                    }
                    editor = editor.child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(div().min_w(px(220.)).text_sm().child(format!(
                                "{}. {}",
                                j + 1,
                                crate::macros::step_label(step)
                            )))
                            .child(moves)
                            .child(
                                Button::new(SharedString::from(format!("macro-delay-less-{j}")))
                                    .label("−10 ms")
                                    .disabled(busy || step.delay_ms == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.macro_engine.edit(|l| {
                                            l.macros[i].steps[j].delay_ms =
                                                l.macros[i].steps[j].delay_ms.saturating_sub(10)
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(format!("{} ms", step.delay_ms))
                            .child(
                                Button::new(SharedString::from(format!("macro-delay-more-{j}")))
                                    .label("+10 ms")
                                    .disabled(busy || step.delay_ms >= 60_000)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.macro_engine.edit(|l| {
                                            l.macros[i].steps[j].delay_ms =
                                                (l.macros[i].steps[j].delay_ms + 10).min(60_000)
                                        });
                                        cx.notify();
                                    })),
                            ),
                    );
                }
            }
        }
        div().id("macros-page").size_full().overflow_y_scroll().p(px(PAGE_PAD)).child(
            div().flex().flex_col().gap_4().max_w(px(1000.))
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Macros"))
                .child(div().text_sm().child(format!("Active set: {active_name}. Editing: {scope_name}.")))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Opening a detected game switches its macro set and this editor. Closing it restores another running game's set, or Default. The newest detected game wins when several run. Open any game in Games, then choose Macros to configure it."))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Windows software macros run while OpenMouse is in the tray. F8 saves a recording. Escape cancels recording or stops playback. Mouse triggers also perform their normal Back/Forward action. Extra G-keys, wheel movement and cursor movement are not recorded."))
                .child(scopes)
                .child(h_flex().gap_3().items_center().flex_wrap()
                    .child(Switch::new("macro-enable").checked(state.library.enabled).disabled(!cfg!(windows) || busy).on_change(cx.listener(|this, on: &bool, _, cx| { this.macro_engine.edit(|l| l.enabled = *on); cx.notify(); })))
                    .child("Enable automatic triggers")
                    .child(Button::new("macro-stop").label("Stop / cancel").on_click(cx.listener(|this, _, _, cx| { this.macro_engine.stop(); cx.notify(); })))
                    .child(Button::new("macro-add").label("Add macro").disabled(busy || state.library.macros.len() >= 64).on_click(cx.listener(move |this, _, _, cx| {
                        let library = this.macro_engine.snapshot().library;
                        let trigger = (0x70..=0x7b).filter(|k| *k != 0x77).map(|key| Trigger::Shortcut { key }).chain([Trigger::Mouse { button: 4 }, Trigger::Mouse { button: 5 }]).find(|t| !library.macros.iter().any(|m| m.game_id == scope && &m.trigger == t));
                        if let Some(trigger) = trigger { let i = library.macros.len(); this.macro_engine.edit(|l| l.macros.push(Macro { game_id: scope.clone(), name: format!("Macro {}", i + 1), trigger, repeats: 1, steps: Vec::new() })); this.macro_selected = Some(i); }
                        cx.notify();
                    }))))
                .child(div().text_sm().child(state.message))
                .child(list).child(editor)
        ).into_any_element()
    }
}
