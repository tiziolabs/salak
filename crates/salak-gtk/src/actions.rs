//! Window and application actions, and their keyboard shortcuts.

use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use gtk::glib::{self, clone};

use crate::window::Window;

/// Shortcuts that work in any context. Single-letter keys are handled by the
/// window itself, so that they never fire while typing in a text field.
/// `Ctrl+PageUp` and `Ctrl+PageDown` are built into `AdwTabView`.
const ACCELS: &[(&str, &[&str])] = &[
    ("win.open-file", &["<Ctrl>o"]),
    ("win.open-folder", &["<Ctrl><Shift>o"]),
    ("win.close-tab", &["<Ctrl>w"]),
    ("win.reload", &["F5", "<Ctrl>r"]),
    ("win.toggle-sidebar", &["<Ctrl>b"]),
    ("win.help('user-guide')", &["F1"]),
    ("app.quit", &["<Ctrl>q"]),
];

pub fn install(window: &Rc<Window>, app: &adw::Application) {
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    app.add_action_entries([quit]);

    let entries = [
        gio::ActionEntry::builder("open-file")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.pick(false)
            ))
            .build(),
        gio::ActionEntry::builder("open-folder")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.pick(true)
            ))
            .build(),
        gio::ActionEntry::builder("close-tab")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.close_tab()
            ))
            .build(),
        gio::ActionEntry::builder("toggle-sidebar")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| {
                    window.split.set_show_sidebar(!window.split.shows_sidebar())
                }
            ))
            .build(),
        // Placeholders until the tasks of phase 6.
        gio::ActionEntry::builder("reload")
            .activate(|_: &adw::ApplicationWindow, _, _| eprintln!("salak: reload: not yet"))
            .build(),
        gio::ActionEntry::builder("help")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|_: &adw::ApplicationWindow, _, page| {
                let page = page
                    .and_then(|page| page.get::<String>())
                    .unwrap_or_default();
                eprintln!("salak: help {page}: not yet");
            })
            .build(),
        gio::ActionEntry::builder("about")
            .activate(|_: &adw::ApplicationWindow, _, _| eprintln!("salak: about: not yet"))
            .build(),
    ];
    window.win.add_action_entries(entries);

    for (action, accels) in ACCELS {
        app.set_accels_for_action(action, accels);
    }
}
