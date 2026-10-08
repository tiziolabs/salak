//! Single-letter keys. They are not accelerators, which would fire while
//! typing in a text field: the window looks at where the focus is first.

use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk::{Key, ModifierType};
use gtk::glib;

use crate::window::{Focus, Window};

/// Pixels scrolled by one press of an arrow, `j` or `k`.
const STEP: f64 = 48.0;

pub fn install(window: &Rc<Window>) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(window);
    controller.connect_key_pressed(move |_, key, _, state| {
        let handled = weak
            .upgrade()
            .is_some_and(|window| pressed(&window, key, state));
        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.win.add_controller(controller);
}

fn pressed(window: &Rc<Window>, key: Key, state: ModifierType) -> bool {
    let others = ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK;
    if state.intersects(others) {
        return false;
    }
    let focus = window.focus_context();
    if focus == Focus::Other {
        return false;
    }
    match key {
        Key::r => window.reload(),
        Key::b => window.toggle_sidebar(),
        Key::slash => window.find(),
        Key::Escape if window.banner_revealed() => window.dismiss_banner(),
        Key::Tab if !state.contains(ModifierType::SHIFT_MASK) => {
            if focus == Focus::Tree || !window.sidebar_shown() {
                window.focus_document();
            } else {
                window.tree.focus();
            }
        }
        _ if focus == Focus::Tree => return window.tree.key(key),
        _ => return scroll(window, key),
    }
    true
}

/// The keys of the document.
fn scroll(window: &Window, key: Key) -> bool {
    let Some(document) = window.document() else {
        return false;
    };
    match key {
        Key::j | Key::Down => document.scroll_by(STEP),
        Key::k | Key::Up => document.scroll_by(-STEP),
        Key::d => document.scroll_page(0.5),
        Key::u => document.scroll_page(-0.5),
        Key::g | Key::Home => document.scroll_edge(true),
        Key::G | Key::End => document.scroll_edge(false),
        _ => return false,
    }
    true
}
