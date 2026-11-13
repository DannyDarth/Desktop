use super::*;
impl AppView {
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
                .spawn(async move { crate::games::custom::add(&path) })
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
                    v.macro_scope = Some(added.id);
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
