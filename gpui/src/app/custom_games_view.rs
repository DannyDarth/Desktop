use super::*;
impl AppView {
    pub fn refresh_game_art(&mut self, cx: &mut Context<Self>) {
        for fresh in games::load_catalog() {
            if let Some(game) = self.games.iter_mut().find(|g| g.id == fresh.id) {
                game.profile_image = fresh.profile_image;
                game.steam_app_id = fresh.steam_app_id;
            }
        }
        cx.notify();
    }
    fn fetch_game_cover(&mut self, id: String, force: bool, cx: &mut Context<Self>) {
        if self.game_art_busy {
            return;
        }
        self.game_art_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { games::auto_cover(&id, force) })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.game_art_busy = false;
                v.refresh_game_art(cx);
                if force {
                    match result {
                        Ok(()) => v.show_toast("Steam artwork updated.", false, cx),
                        Err(e) => v.show_toast(e, true, cx),
                    }
                }
            });
        })
        .detach();
    }
    fn choose_game_image(&mut self, id: String, cx: &mut Context<Self>) {
        if self.game_art_busy {
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a PNG, JPEG or BMP image".into()),
        });
        self.game_art_busy = true;
        cx.spawn(async move |view, cx| {
            let result = match chosen.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        cx.background_executor()
                            .spawn(async move { games::profile_art::choose_image(&id, &path) })
                            .await
                    } else {
                        Ok(())
                    }
                }
                Ok(Ok(None)) => Ok(()),
                other => Err(format!("Could not select image: {other:?}")),
            };
            let _ = view.update(cx, |v, cx| {
                v.game_art_busy = false;
                v.refresh_game_art(cx);
                if let Err(e) = result {
                    v.show_toast(e, true, cx);
                }
            });
        })
        .detach();
    }
    pub(super) fn render_game_art_actions(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let choose_id = id.to_owned();
        let fetch_id = id.to_owned();
        h_flex()
            .gap_2()
            .child(
                Button::new("choose-profile-image")
                    .compact()
                    .label("Choose image")
                    .disabled(self.game_art_busy)
                    .on_click(
                        cx.listener(move |v, _, _, cx| v.choose_game_image(choose_id.clone(), cx)),
                    ),
            )
            .child(
                Button::new("find-steam-cover")
                    .compact()
                    .label(if self.game_art_busy {
                        "Loading artwork…"
                    } else {
                        "Find Steam cover"
                    })
                    .disabled(self.game_art_busy)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.fetch_game_cover(fetch_id.clone(), true, cx)
                    })),
            )
            .into_any_element()
    }
    pub(super) fn add_custom_game(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select the game's .exe (not its launcher)".into()),
        });
        cx.spawn(async move |view, cx| {
            let path = match chosen.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                other => {
                    let error = match other {
                        Ok(Err(e)) => e.to_string(),
                        Err(e) => e.to_string(),
                        _ => "Could not open the game picker.".into(),
                    };
                    let _ = view.update(cx, |v, cx| v.show_toast(error, true, cx));
                    return;
                }
            };
            let Some(path) = path else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    let added = crate::games::custom::add(&path)?;
                    games::cache_executable_icon(&added.path);
                    Ok::<_, String>(added)
                })
                .await;
            let _ = view.update(cx, |v, cx| match result {
                Ok(added) => {
                    let previous = std::mem::take(&mut v.games);
                    v.games = games::load_catalog();
                    for game in &mut v.games {
                        if let Some(old) = previous.iter().find(|old| old.id == game.id) {
                            game.running = old.running;
                            game.installed = old.installed || game.installed;
                        }
                    }
                    v.macro_engine.set_catalog(games::macro_targets(&v.games));
                    v._game_watcher = Some(crate::game_watcher::GameWatcher::start(cx));
                    v.macro_text_edit = None;
                    v.macro_scope = Some(added.id.clone());
                    v.fetch_game_cover(added.id, false, cx);
                    v.macro_selected = None;
                    v.page = Page::Macros;
                    v.show_toast(
                        format!("Added {}. Set up its macros below.", added.name),
                        false,
                        cx,
                    );
                    cx.notify();
                }
                Err(error) => v.show_toast(error, true, cx),
            });
        })
        .detach();
    }
}
