// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use addresses_rs::settings::AppSettings;
use app::AddressBookApp;
use gpui::{App, AppContext as _, Application, Bounds, WindowBounds, WindowOptions, px, size};

mod app;
mod text_field;

use crate::text_field::TextField;

fn main() {
    Application::new().run(|cx: &mut App| {
        text_field::bind_key_actions(cx);
        let (settings, settings_error) = match AppSettings::load() {
            Ok(settings) => (settings, None),
            Err(error) => (AppSettings::default(), Some(error)),
        };
        let search_field =
            cx.new(|cx| TextField::new(cx, "contact-search-field", "Search contacts", ""));
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                let search_field = search_field.clone();
                cx.new(move |cx| {
                    cx.observe_window_appearance(window, |_: &mut AddressBookApp, _, cx| {
                        cx.notify()
                    })
                    .detach();
                    let mut app: AddressBookApp =
                        AddressBookApp::new(settings, settings_error, search_field.clone());
                    cx.observe(&search_field, |this, _, cx| this.on_search_changed(cx))
                        .detach();
                    app.restore_last_database(cx);
                    app
                })
            },
        )
        .expect("Could not open the Addresses window");
        cx.activate(true);
    });
}
