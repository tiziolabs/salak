mod actions;
mod buffer;
mod document;
mod layout;
mod tree;
mod window;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use adw::prelude::*;
use salak_core::cli;
use salak_core::session::Session;

pub const APP_ID: &str = "com.tiziolabs.salak";

fn main() -> ExitCode {
    // Parsed before GTK starts, so that GApplication never sees the arguments.
    let session = match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Command::Help) => {
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Version) => {
            println!("salak {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        // The theme is applied by the next phase.
        Ok(cli::Command::Run { path, theme: _ }) => Session::from_args(path),
        Err(err) => Err(format!("{err}\n\n{}", cli::USAGE)),
    };
    let session = match session {
        Ok(session) => session,
        Err(err) => {
            eprintln!("salak: {err}");
            return ExitCode::from(2);
        }
    };

    // Each invocation opens its own folder, as in the Tauri build, so there
    // is no single instance handing the path over to a running window.
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let session = Rc::new(RefCell::new(Some(session)));
    // The actions only hold weak references to the window.
    let windows = Rc::new(RefCell::new(Vec::new()));
    app.connect_activate(move |app| {
        if let Some(session) = session.borrow_mut().take() {
            windows.borrow_mut().push(window::Window::new(app, session));
        }
    });

    // Empty list: GApplication must not parse the command line itself.
    let status = app.run_with_args(&[] as &[&str]);
    ExitCode::from(status.get())
}
