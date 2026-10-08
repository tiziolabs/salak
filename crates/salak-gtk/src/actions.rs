//! Window and application actions, and their keyboard shortcuts.

use std::cell::RefCell;
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
    ("win.find", &["<Ctrl>f"]),
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
        gio::ActionEntry::builder("find")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.find()
            ))
            .build(),
        gio::ActionEntry::builder("toggle-sidebar")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.toggle_sidebar()
            ))
            .build(),
        gio::ActionEntry::builder("help")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, page| {
                    let page = page
                        .and_then(|page| page.get::<String>())
                        .unwrap_or_default();
                    window.open_help(&page);
                }
            ))
            .build(),
        gio::ActionEntry::builder("reload")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.reload()
            ))
            .build(),
        gio::ActionEntry::builder("about")
            .activate(clone!(
                #[weak]
                window,
                move |_: &adw::ApplicationWindow, _, _| window.about()
            ))
            .build(),
    ];
    window.win.add_action_entries(entries);
    install_tab_menu(window);
    // Nothing to search until a document is open.
    window.set_find_enabled(false);

    for (action, accels) in ACCELS {
        app.set_accels_for_action(action, accels);
    }
}

/// The context menu of the tabs. The actions work on the tab under the
/// pointer, which `setup-menu` reports.
fn install_tab_menu(window: &Rc<Window>) {
    let target: Rc<RefCell<Option<adw::TabPage>>> = Rc::default();
    let group = gio::SimpleActionGroup::new();
    let add = |name: &str, close: fn(&adw::TabView, &adw::TabPage)| {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(clone!(
            #[strong]
            target,
            #[weak(rename_to = tabs)]
            window.tabs,
            move |_, _| {
                if let Some(page) = target.borrow().as_ref() {
                    close(&tabs, page);
                }
            }
        ));
        group.add_action(&action);
        action
    };
    add("close", |tabs, page| tabs.close_page(page));
    let others = add("close-others", |tabs, page| tabs.close_other_pages(page));
    let right = add("close-right", |tabs, page| tabs.close_pages_after(page));
    let left = add("close-left", |tabs, page| tabs.close_pages_before(page));
    window.win.insert_action_group("tab", Some(&group));

    let menu = gio::Menu::new();
    menu.append(Some("Close This Tab"), Some("tab.close"));
    menu.append(Some("Close Other Tabs"), Some("tab.close-others"));
    menu.append(Some("Close Tabs to the Right"), Some("tab.close-right"));
    menu.append(Some("Close Tabs to the Left"), Some("tab.close-left"));
    window.tabs.set_menu_model(Some(&menu));
    // Entries that would close nothing are disabled.
    window.tabs.connect_setup_menu(move |tabs, page| {
        *target.borrow_mut() = page.cloned();
        if let Some(page) = page {
            let (position, count) = (tabs.page_position(page), tabs.n_pages());
            others.set_enabled(count > 1);
            right.set_enabled(position + 1 < count);
            left.set_enabled(position > 0);
        }
    });
}
