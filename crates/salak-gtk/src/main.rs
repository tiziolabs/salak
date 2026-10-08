mod actions;
mod buffer;
mod document;
#[cfg(feature = "highlight")]
mod highlight;
mod keys;
mod layout;
mod monitor;
mod search;
mod theme;
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
    let launch = match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Command::Help) => {
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Version) => {
            println!("salak {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Run { path, theme }) => {
            // An explicit theme must exist, the default one is optional.
            let theme = match theme {
                Some(theme) => salak_core::theme::explicit_path(&theme).map(Some),
                None => Ok(salak_core::theme::default_path()),
            };
            theme.and_then(|theme| Ok((Session::from_args(path)?, theme)))
        }
        Err(err) => Err(format!("{err}\n\n{}", cli::USAGE)),
    };
    let (session, theme) = match launch {
        Ok(launch) => launch,
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

    let launch = Rc::new(RefCell::new(Some((session, theme))));
    // The actions only hold weak references to the window.
    let windows = Rc::new(RefCell::new(Vec::new()));
    #[cfg(feature = "highlight")]
    app.connect_startup(|_| sourceview5::init());
    app.connect_activate(move |app| {
        if let Some((session, theme)) = launch.borrow_mut().take() {
            windows
                .borrow_mut()
                .push(window::Window::new(app, session, theme));
        }
    });

    // Empty list: GApplication must not parse the command line itself.
    let status = app.run_with_args(&[] as &[&str]);
    ExitCode::from(i32::from(status) as u8)
}
