use super::*;
use crate::macros::{Macro, MacroMode, Section, Trigger};

#[derive(Clone)]
struct MacroStepDrag {
    owner: EntityId,
    macro_index: usize,
    from: usize,
    section: Section,
    steps: std::sync::Arc<[crate::macros::Step]>,
    label: String,
}
impl Render for MacroStepDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().primary)
            .rounded(cx.theme().radius)
            .child(self.label.clone())
    }
}

impl AppView {
    fn macro_drop_zone(
        &self,
        macro_index: usize,
        gap: usize,
        section: Section,
        len: usize,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity_id();
        div()
            .id(SharedString::from(format!("macro-gap-{macro_index}-{gap}")))
            .w_full()
            .h(px(10.))
            .can_drop(move |any, _, _| {
                !busy
                    && any.downcast_ref::<MacroStepDrag>().is_some_and(|drag| {
                        drag.owner == owner
                            && drag.macro_index == macro_index
                            && drag.section == section
                            && crate::macros::drop_target(drag.from, gap, len).is_some()
                    })
            })
            .drag_over::<MacroStepDrag>(move |style, drag, _, cx| {
                if !busy
                    && drag.owner == owner
                    && drag.macro_index == macro_index
                    && drag.section == section
                    && crate::macros::drop_target(drag.from, gap, len).is_some()
                {
                    style.bg(cx.theme().primary)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(move |this, drag: &MacroStepDrag, _, cx| {
                if busy
                    || drag.owner != cx.entity_id()
                    || drag.macro_index != macro_index
                    || drag.section != section
                {
                    return;
                }
                let state = this.macro_engine.snapshot();
                if state.playing || state.recording.is_some() || state.assigning.is_some() {
                    return;
                }
                let Some(m) = state.library.macros.get(macro_index) else {
                    return;
                };
                if m.section(section) != drag.steps.as_ref() {
                    return;
                }
                if let Some(to) =
                    crate::macros::drop_target(drag.from, gap, m.section(section).len())
                {
                    this.macro_engine
                        .move_section_step(macro_index, section, drag.from, to);
                    cx.notify();
                }
            }))
            .into_any_element()
    }
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
        let busy = state.playing || state.recording.is_some() || state.assigning.is_some();
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
        let mut activation = div().flex().flex_col().gap_2();
        if let Some(game_id) = scope.clone() {
            let focus_only = state.library.focus_only.contains(&game_id);
            let mut options = h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .child(div().text_sm().child("Activate game macros:"));
            for (focused, label) in [(false, "While running"), (true, "While focused")] {
                let id = game_id.clone();
                options = options.child(self.chip(
                    format!("macro-focus-{focused}"),
                    label.into(),
                    focus_only == focused,
                    cx,
                    move |this, cx| {
                        if busy || !cfg!(windows) {
                            return;
                        }
                        this.macro_engine.edit(|l| {
                            if focused {
                                l.focus_only.insert(id.clone());
                            } else {
                                l.focus_only.remove(&id);
                            }
                        });
                        cx.notify();
                    },
                ));
            }
            activation = activation.child(options).child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(if focus_only { "Alt+Tab switches away from this macro set and stops its playback. Returning to the game reactivates the set." } else { "Keeps this game's macro set active while it runs, including after Alt+Tab." }));
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
                        m.event_count()
                    }
                ),
                selected == Some(i),
                cx,
                move |this, cx| {
                    this.macro_engine.select_section(Section::Repeat);
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
            let section = if m.mode == MacroMode::Sequence {
                state.section
            } else {
                Section::Repeat
            };
            let steps = m.section(section);
            let playback_error = state.library.playback_error(i);
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
                                .disabled(!cfg!(windows) || busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.macro_engine.record(i);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("macro-play")
                                .label("Play once")
                                .disabled(
                                    !cfg!(windows)
                                        || busy
                                        || m.event_count() == 0
                                        || playback_error.is_some()
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
                .child(div().text_sm().child("Activation"));
            let trigger_row = h_flex()
                .gap_3()
                .items_center()
                .flex_wrap()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(m.trigger.label()),
                )
                .child(
                    Button::new("macro-assign-trigger")
                        .compact()
                        .label(if state.assigning == Some(i) {
                            "Waiting for input…"
                        } else {
                            "Assign input"
                        })
                        .disabled(!cfg!(windows) || busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.macro_engine.assign_trigger(i);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Press a key, shortcut or mouse button. Escape cancels."),
                );
            editor = editor.child(trigger_row);
            let mut modes = h_flex().gap_2().flex_wrap();
            for mode in [
                MacroMode::NoRepeat,
                MacroMode::WhileHeld,
                MacroMode::Toggle,
                MacroMode::Sequence,
            ] {
                modes = modes.child(self.chip(
                    format!("macro-mode-{mode:?}"),
                    mode.label().into(),
                    m.mode == mode,
                    cx,
                    move |this, cx| {
                        if busy {
                            return;
                        }
                        this.macro_engine.edit(|l| l.macros[i].mode = mode);
                        this.macro_engine.select_section(Section::Repeat);
                        cx.notify();
                    },
                ));
            }
            editor = editor.child(modes);
            let help = match m.mode {
                MacroMode::NoRepeat => "Plays once when you press the activation input.",
                MacroMode::WhileHeld => "Repeats while the activation input is held. Releasing it stops playback.",
                MacroMode::Toggle => "Press the activation input to start repeating. Press it again to stop.",
                MacroMode::Sequence => "On press plays once, While held repeats, and On release plays once when you let go.",
                MacroMode::Count => "This existing macro uses its saved repeat count. Choose a type above to change it.",
            };
            editor = editor.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(help),
            );
            if m.mode == MacroMode::Count {
                editor = editor.child(
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
            }
            if m.mode == MacroMode::Sequence {
                let mut sections = h_flex().gap_2().flex_wrap();
                for phase in [Section::Press, Section::Repeat, Section::Release] {
                    sections = sections.child(self.chip(
                        format!("macro-section-{phase:?}"),
                        format!("{} · {} events", phase.label(), m.section(phase).len()),
                        section == phase,
                        cx,
                        move |this, cx| {
                            if !busy {
                                this.macro_engine.select_section(phase);
                                cx.notify();
                            }
                        },
                    ));
                }
                editor = editor.child(sections).child(
                    div()
                        .text_sm()
                        .child(format!("Recording and editing: {}", section.label())),
                );
            }
            if let Some(error) = playback_error {
                editor = editor.child(div().text_sm().text_color(cx.theme().danger).child(
                    format!("Order saved. Playback is paused until the order is complete: {error}"),
                ));
            }
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
                    .child("Drag a row to reorder it. Drop on the green line. Delays move with their events."));
                let dragged_steps: std::sync::Arc<[crate::macros::Step]> = steps.to_vec().into();
                let mut rows = div().flex().flex_col();
                for (j, step) in steps.iter().enumerate() {
                    rows = rows.child(self.macro_drop_zone(i, j, section, steps.len(), busy, cx));
                    let payload = MacroStepDrag {
                        owner: cx.entity_id(),
                        macro_index: i,
                        from: j,
                        section,
                        steps: dragged_steps.clone(),
                        label: crate::macros::step_label(step),
                    };
                    rows = rows.child(
                        h_flex()
                            .id(SharedString::from(format!("macro-event-{j}")))
                            .gap_3()
                            .items_center()
                            .px_3()
                            .py_2()
                            .w_full()
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(cx.theme().radius)
                            .when(!busy, |row| {
                                row.cursor_grab().on_drag(payload, |drag, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.new(|_| drag.clone())
                                })
                            })
                            .child(div().text_color(cx.theme().muted_foreground).child("⠿"))
                            .child(
                                div()
                                    .w(px(28.))
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{}", j + 1)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .child(crate::macros::step_label(step)),
                            )
                            .child(
                                Button::new(SharedString::from(format!("macro-delay-less-{j}")))
                                    .compact()
                                    .label("−")
                                    .tooltip("Reduce delay by 10 ms")
                                    .disabled(busy || step.delay_ms == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.macro_engine.edit(|l| {
                                            l.macros[i].section_mut(section)[j].delay_ms =
                                                l.macros[i].section_mut(section)[j]
                                                    .delay_ms
                                                    .saturating_sub(10)
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .w(px(75.))
                                    .text_sm()
                                    .child(format!("{} ms", step.delay_ms)),
                            )
                            .child(
                                Button::new(SharedString::from(format!("macro-delay-more-{j}")))
                                    .compact()
                                    .label("+")
                                    .tooltip("Increase delay by 10 ms")
                                    .disabled(busy || step.delay_ms >= 60_000)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.macro_engine.edit(|l| {
                                            l.macros[i].section_mut(section)[j].delay_ms =
                                                (l.macros[i].section_mut(section)[j].delay_ms + 10)
                                                    .min(60_000)
                                        });
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                rows = rows.child(self.macro_drop_zone(
                    i,
                    steps.len(),
                    section,
                    steps.len(),
                    busy,
                    cx,
                ));
                editor = editor.child(rows);
            }
        }
        div().id("macros-page").size_full().overflow_y_scroll().p(px(PAGE_PAD)).child(
            div().flex().flex_col().gap_4().max_w(px(1000.))
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Macros"))
                .child(div().text_sm().child(format!("Active set: {active_name}. Editing: {scope_name}.")))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Game sets switch automatically when games open or close. Configure another game from Games → Macros."))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("F8 saves a recording. Escape stops or cancels. Triggers keep their normal input action. Dedicated G-keys, extra vendor buttons and cursor/wheel movement need separate support."))
                .child(scopes).child(activation)
                .child(h_flex().gap_3().items_center().flex_wrap()
                    .child(Switch::new("macro-enable").checked(state.library.enabled).disabled(!cfg!(windows) || busy).on_change(cx.listener(|this, on: &bool, _, cx| { this.macro_engine.edit(|l| l.enabled = *on); cx.notify(); })))
                    .child("Enable automatic triggers")
                    .child(Button::new("macro-stop").label("Stop / cancel").on_click(cx.listener(|this, _, _, cx| { this.macro_engine.stop(); cx.notify(); })))
                    .child(Button::new("macro-add").label("Add macro").disabled(busy || state.library.macros.len() >= 64).on_click(cx.listener(move |this, _, _, cx| {
                        let library = this.macro_engine.snapshot().library;
                        let trigger = (0x70..=0x7b).filter(|k| *k != 0x77).map(|key| Trigger::Shortcut { key }).chain([Trigger::Mouse { button: 4 }, Trigger::Mouse { button: 5 }]).chain((0x7c..=0x87).map(|code| Trigger::Key { code, modifiers: 0 })).chain((0x41..=0x5a).map(|code| Trigger::Key { code, modifiers: 0 })).find(|t| !library.macros.iter().any(|m| m.game_id == scope && m.trigger.same_input(t)));
                        if let Some(trigger) = trigger { let i = library.macros.len(); this.macro_engine.edit(|l| l.macros.push(Macro { mode: MacroMode::NoRepeat, on_press: vec![], on_release: vec![], game_id: scope.clone(), name: format!("Macro {}", i + 1), trigger, repeats: 1, steps: Vec::new() })); this.macro_selected = Some(i); }
                        cx.notify();
                    }))))
                .child(div().text_sm().child(state.message))
                .child(list).child(editor)
        ).into_any_element()
    }
}
