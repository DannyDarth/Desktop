use super::*;
impl AppView {
    pub(super) fn check_firmware_source(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(tile) = self.devices.tiles.iter().find(|t| t.key == key) else {
            return;
        };
        let Some(source) = crate::firmware::source_for(&tile.brand, &tile.name) else {
            return;
        };
        let check_key = format!("{}|{}", key, tile.name.to_ascii_lowercase());
        if matches!(
            self.firmware_checks.get(&check_key),
            Some(crate::firmware::CheckState::Checking)
        ) {
            return;
        }
        self.firmware_checks
            .insert(check_key.clone(), crate::firmware::CheckState::Checking);
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::firmware::check(source) })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.firmware_checks.insert(check_key, result);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn render_firmware(
        &self,
        tile: &Tile,
        r: &DeviceReadout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_4()
            .w_full()
            .min_w_0()
            .max_w(px(720.))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Firmware"),
            );
        let value = |label: &str, text: String| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg)
                        .child(label.to_owned()),
                )
                .child(div().text_sm().text_color(muted).child(text))
        };
        panel = panel.child(value(
            "Installed device firmware",
            if r.firmware_info.versions.is_empty() {
                "Not reported by this device's driver".into()
            } else {
                r.firmware_info.versions.join(" · ")
            },
        ));
        panel = panel.child(value(
            "USB receiver firmware",
            if !r.firmware_info.receiver_connected {
                "No USB receiver identified for this connection".into()
            } else {
                r.firmware_info.receiver_version.clone().unwrap_or_else(|| {
                    "Receiver connected; its firmware version is not reported by this driver".into()
                })
            },
        ));
        for component in &r.firmware_info.details {
            panel = panel.child(value(
                &component.component,
                format!(
                    "{} {} (build {}){}",
                    component.prefix,
                    component.version,
                    component.build,
                    if component.active { " — active" } else { "" }
                ),
            ));
        }
        let identity = format!("{}|{}", tile.key, tile.name.to_ascii_lowercase());
        let release = match self.firmware_checks.get(&identity) {
            Some(crate::firmware::CheckState::Checked { release, .. }) => release.as_ref(),
            _ => None,
        };
        match crate::firmware::availability(&identity,&r.firmware_info,release) {
            crate::firmware::Availability::UpdateAvailable{version,url}=> {
                panel=panel.child(value("Available firmware",version))
                    .child(Button::new("firmware-update-available").label("Update Available").on_click(cx.listener(move |_,_,_,cx|cx.open_url(&url))));
            },
            crate::firmware::Availability::UpToDate=>panel=panel.child(value("Update availability","Installed version matches or is newer than the verified official release.".into())),
            crate::firmware::Availability::Unknown=>panel=panel.child(value("Update availability","Unknown — no matching firmware version and direct official download have been verified for this exact device.".into())),
        }
        if let Some(source) = crate::firmware::source_for(&tile.brand, &tile.name) {
            let key = tile.key.clone();
            let checking = matches!(
                self.firmware_checks.get(&identity),
                Some(crate::firmware::CheckState::Checking)
            );
            let status = match self.firmware_checks.get(&identity) {
                Some(crate::firmware::CheckState::Checking) => {
                    "Checking the official product source…".to_owned()
                }
                Some(crate::firmware::CheckState::Checked {
                    at, source_updated, ..
                }) => {
                    let age = at.elapsed().as_secs() / 60;
                    let when = if age == 0 {
                        "just now".into()
                    } else {
                        format!("{age} minute(s) ago")
                    };
                    format!("Official product source checked {when}.{} This confirms the source is reachable; it does not confirm that your firmware is current.",source_updated.as_ref().map(|date|format!(" Product article updated {date}.")).unwrap_or_default())
                }
                Some(crate::firmware::CheckState::Failed(error)) => error.clone(),
                None => "Official product source has not been checked this session.".into(),
            };
            panel = panel.child(div().text_sm().text_color(muted).child(status));
            panel = panel.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("firmware-check")
                            .label(if checking {
                                "Checking…"
                            } else {
                                "Check official source"
                            })
                            .disabled(checking)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.check_firmware_source(key.clone(), cx)
                            })),
                    )
                    .child(
                        Button::new("firmware-official-support")
                            .label("Official support / downloads")
                            .on_click(
                                cx.listener(move |_, _, _, cx| cx.open_url(source.product_url)),
                            ),
                    ),
            );
            if let Some((url, label)) = source.release_notes {
                panel = panel
                    .child(value("Published firmware release notes", label.into()))
                    .child(
                        Button::new("firmware-notes")
                            .label("Read official release notes")
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url))),
                    );
            } else {
                panel=panel.child(div().text_sm().text_color(muted).child("No model-specific firmware release-note or standalone firmware download link has been verified for this model."));
            }
        } else {
            panel=panel.child(div().text_sm().text_color(muted).child("No verified official update source is configured for this model yet. Installed information above comes directly from its driver."));
        }
        panel.child(div().text_sm().text_color(muted).child("Firmware installation is not available in OpenMouse. Checking sources does not download or install firmware."))
            .child({let key=tile.key.clone();Button::new("refresh-firmware-readout").label("Refresh installed information").disabled(self.devices.writing.is_some()).on_click(cx.listener(move |v,_,_,cx|v.connect_tile(&key,true,cx)))})
            .into_any_element()
    }
}
