use super::*;
impl AppView {
    pub(crate) fn profile_ids_with_macros(&self) -> std::collections::HashSet<String> {
        self.macro_engine
            .snapshot()
            .library
            .macros
            .iter()
            .filter_map(|m| m.game_id.clone())
            .collect()
    }
    pub(crate) fn apply_profile_cleanup(
        &mut self,
        catalog: Vec<games::Game>,
        removed: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        if removed.is_empty() {
            return;
        }
        let previous = std::mem::replace(&mut self.games, catalog);
        for game in &mut self.games {
            if let Some(old) = previous.iter().find(|g| g.id == game.id) {
                game.running = old.running;
                game.installed |= old.installed;
            }
        }
        for (old, keep) in &removed {
            self.profiles.consolidate_duplicate(old, keep);
            if self.macro_scope.as_deref() == Some(old.as_str()) {
                self.macro_scope = Some(keep.clone());
                self.macro_selected = None;
                self.macro_text_edit = None;
            }
            if self.profile_editor.as_deref() == Some(old.as_str()) {
                self.profile_editor = Some(keep.clone());
            }
        }
        self.profiles.save();
        self.macro_engine
            .set_catalog(games::macro_targets(&self.games));
        self.show_toast(
            format!(
                "Removed {} empty duplicate profile(s). Profiles containing macros were kept.",
                removed.len()
            ),
            false,
            cx,
        );
        cx.notify();
    }
    pub(crate) fn report_game_discovery_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.show_toast(
            format!("Could not add game automatically: {error}"),
            true,
            cx,
        );
    }
    pub(crate) fn apply_discovered_game(
        &mut self,
        found: games::discovery::Discovered,
        cx: &mut Context<Self>,
    ) {
        let id = found
            .game
            .catalog_id
            .clone()
            .unwrap_or(found.game.id.clone());
        let added = !self.games.iter().any(|g| g.id == id);
        let previous = std::mem::replace(&mut self.games, found.catalog);
        for game in &mut self.games {
            if let Some(old) = previous.iter().find(|g| g.id == game.id) {
                game.running = old.running;
                game.installed |= old.installed;
            }
        }
        if added && found.game.auto_discovered {
            if let Err(error) = self.macro_engine.register_discovered_profile(&id) {
                self.report_game_discovery_error(error, cx);
                return;
            }
        }
        self.macro_engine
            .set_catalog(games::macro_targets(&self.games));
        if let Some(game) = self.games.iter_mut().find(|g| g.id == id) {
            game.running = true;
            game.installed = true;
        }
        let running = self
            .games
            .iter()
            .filter(|g| g.running)
            .map(|g| g.id.clone())
            .collect();
        self.macro_engine.sync_games(running);
        if added {
            let art_id = id.clone();
            cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .spawn(async move {
                        if let Some(game) =
                            games::load_catalog().into_iter().find(|g| g.id == art_id)
                        {
                            if let Some(path) = game.executable_path {
                                games::cache_executable_icon(&path);
                            }
                        }
                        let _ = games::auto_cover(&art_id, false);
                    })
                    .await;
                let _ = view.update(cx, |v, cx| v.refresh_game_art(cx));
            })
            .detach();
            let name = self
                .games
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| "Game".into());
            self.show_toast(
                format!("Added {name}. Its macro profile activates while focused."),
                false,
                cx,
            );
        }
        cx.notify();
    }
    fn delete_game_profile(&mut self, id: String, cx: &mut Context<Self>) {
        if self.game_art_busy {
            return;
        }
        if self.game_delete_pending.as_deref() != Some(id.as_str()) {
            self.game_delete_pending = Some(id);
            cx.notify();
            return;
        }
        if self.profile_inflight.iter().any(|(game, _)| game == &id)
            || self.devices.writing.is_some()
        {
            self.show_toast(
                "Wait for the device settings change to finish, then retry.",
                true,
                cx,
            );
            return;
        }
        let Some(game) = self.games.iter().find(|g| g.id == id).cloned() else {
            return;
        };
        self.game_art_busy = true;
        let delete = self.macro_engine.prepare_game_profile_delete(id.clone());
        cx.spawn(async move |view,cx| {
            let result=cx.background_executor().spawn(async move {
                games::deleted::delete(&game)?;
                if let Err(error)=delete() {let _=games::deleted::restore(&game.id);return Err(error);}
                Ok::<_,String>(games::load_catalog())
            }).await;
            let _=view.update(cx,|v,cx| {
                v.game_art_busy=false;v.game_delete_pending=None;
                match result {
                    Ok(mut catalog)=> {
                        for game in &mut catalog {if let Some(old)=v.games.iter().find(|g|g.id==game.id) {game.running=old.running;game.installed |= old.installed;}}
                        v.on_game_closed(&id,cx);v.profiles.delete_game(&id);v.profiles.save();
                        v.games=catalog;v.macro_engine.set_catalog(games::macro_targets(&v.games));
                        v.macro_scope=None;v.macro_selected=None;v.macro_text_edit=None;v.profile_editor=None;
                        v._game_watcher=Some(crate::game_watcher::GameWatcher::start(cx));
                        v.show_toast("Profile and its macros deleted. Automatic discovery will not recreate it. Use Add game to restore it manually.",false,cx);
                    },Err(error)=>v.show_toast(error,true,cx),
                }
                cx.notify();
            });
        }).detach();
    }
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
        let delete_id = id.to_owned();
        let confirming = self.game_delete_pending.as_deref() == Some(id);
        h_flex()
            .gap_2()
            .flex_wrap()
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
            .child(
                Button::new("delete-game-profile")
                    .compact()
                    .label(if confirming {
                        "Confirm delete — macros will be erased"
                    } else {
                        "Delete profile"
                    })
                    .disabled(self.game_art_busy)
                    .on_click(
                        cx.listener(move |v, _, _, cx| {
                            v.delete_game_profile(delete_id.clone(), cx)
                        }),
                    ),
            )
            .children(confirming.then(|| {
                Button::new("cancel-delete-profile")
                    .compact()
                    .disabled(self.game_art_busy)
                    .label("Cancel")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.game_delete_pending = None;
                        cx.notify();
                    }))
            }))
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
                    games::deleted::restore(added.catalog_id.as_deref().unwrap_or(&added.id))?;
                    games::cache_executable_icon(&added.path);
                    Ok::<_, String>(added)
                })
                .await;
            let _ = view.update(cx, |v, cx| match result {
                Ok(mut added) => {
                    added.id = added.catalog_id.clone().unwrap_or(added.id);
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
