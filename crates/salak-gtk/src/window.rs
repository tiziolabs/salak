//! The main window: sidebar tree, tabs and the welcome page.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use salak_core::files::{self, MARKDOWN_EXTENSIONS};
use salak_core::help;
use salak_core::markdown::Link;
use salak_core::session::{window_title, Session};

use crate::actions;
use crate::document::{self, Document};
use crate::tree::Tree;

pub struct Window {
    pub win: adw::ApplicationWindow,
    session: RefCell<Session>,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    pub split: adw::OverlaySplitView,
    folder: gtk::Label,
    tree: Rc<Tree>,
    tabs: adw::TabView,
    /// Banner shown when the open file changed on disk.
    #[allow(dead_code)]
    banner: adw::Banner,
    /// Only one file dialog at a time.
    picking: Cell<bool>,
}

/// Shown by sway in the title bar / tab of the container.
fn under_tiling_wm() -> bool {
    ["SWAYSOCK", "I3SOCK", "HYPRLAND_INSTANCE_SIGNATURE"]
        .iter()
        .any(|var| std::env::var_os(var).is_some())
}

fn primary_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let files = gio::Menu::new();
    files.append(Some("Open File…"), Some("win.open-file"));
    files.append(Some("Open Folder…"), Some("win.open-folder"));
    files.append(Some("Close Tab"), Some("win.close-tab"));
    menu.append_section(None, &files);
    let help = gio::Menu::new();
    help.append(Some("User Guide"), Some("win.help('user-guide')"));
    help.append(Some("Theming Guide"), Some("win.help('theming')"));
    help.append(Some("About Salak"), Some("win.about"));
    menu.append_section(None, &help);
    let quit = gio::Menu::new();
    quit.append(Some("Quit"), Some("app.quit"));
    menu.append_section(None, &quit);
    menu
}

/// The welcome page, shown while no folder is open.
fn welcome() -> adw::StatusPage {
    let open_file = gtk::Button::builder()
        .label("Open File…")
        .action_name("win.open-file")
        .tooltip_text("Ctrl+O")
        .css_classes(["pill", "suggested-action"])
        .build();
    let open_folder = gtk::Button::builder()
        .label("Open Folder…")
        .action_name("win.open-folder")
        .tooltip_text("Ctrl+Shift+O")
        .css_classes(["pill"])
        .build();
    let buttons = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .build();
    buttons.append(&open_file);
    buttons.append(&open_folder);
    let guide = gtk::Button::builder()
        .label("Read the user guide (F1)")
        .action_name("win.help")
        .action_target(&"user-guide".to_variant())
        .css_classes(["flat"])
        .halign(gtk::Align::Center)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .build();
    content.append(&buttons);
    content.append(&guide);
    adw::StatusPage::builder()
        .icon_name(crate::APP_ID)
        .title("Salak")
        .description("Open a Markdown file, or a folder to browse its files.")
        .child(&content)
        .build()
}

impl Window {
    pub fn new(app: &adw::Application, session: Session) -> Rc<Window> {
        install_css();
        document::install_css();

        let tree = Tree::new();
        let tree_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(tree.widget())
            .build();
        let folder = gtk::Label::builder()
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["heading"])
            .build();
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar.append(&folder);
        sidebar.append(&tree_scroll);

        let tabs = adw::TabView::new();
        let tab_bar = adw::TabBar::builder().view(&tabs).autohide(false).build();
        let banner = adw::Banner::builder()
            .title("This file has changed on disk.")
            .button_label("Reload")
            .action_name("win.reload")
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&tab_bar);
        content.append(&banner);
        content.append(&tabs);
        tabs.set_vexpand(true);

        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .sidebar_width_fraction(0.25)
            .min_sidebar_width(200.0)
            .max_sidebar_width(420.0)
            .build();
        let stack = gtk::Stack::new();
        stack.add_named(&welcome(), Some("welcome"));
        stack.add_named(&split, Some("main"));

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));

        let header = adw::HeaderBar::new();
        let sidebar_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Toggle Sidebar (Ctrl+B)")
            .build();
        split
            .bind_property("show-sidebar", &sidebar_toggle, "active")
            .bidirectional()
            .sync_create()
            .build();
        header.pack_start(&sidebar_toggle);
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .primary(true)
            .menu_model(&primary_menu())
            .build();
        header.pack_end(&menu_button);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&toasts));

        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title(window_title(None))
            .default_width(1000)
            .default_height(700)
            .content(&toolbar)
            .build();
        if under_tiling_wm() {
            win.set_decorated(false);
            header.set_show_start_title_buttons(false);
            header.set_show_end_title_buttons(false);
        }

        let this = Rc::new(Window {
            win,
            session: RefCell::new(session),
            toasts,
            stack,
            split,
            folder,
            tree,
            tabs,
            banner,
            picking: Cell::new(false),
        });
        this.banner.set_revealed(false);
        actions::install(&this, app);

        this.tree.connect_open(glib::clone!(
            #[weak]
            this,
            move |path| this.open_document(path)
        ));
        this.tabs.connect_selected_page_notify(glib::clone!(
            #[weak]
            this,
            move |_| this.update_selection()
        ));

        // The root and the initial file come from the command line.
        let (root, initial) = {
            let session = this.session.borrow();
            (session.root.clone(), session.initial.clone())
        };
        if let Some(root) = root {
            glib::spawn_future_local(glib::clone!(
                #[strong]
                this,
                async move {
                    this.show_root(root).await;
                    if let Some(file) = initial {
                        this.open_document(file.clone());
                        this.tree.reveal(&file).await;
                    }
                }
            ));
        } else {
            this.stack.set_visible_child_name("welcome");
        }
        this.win.present();
        this
    }

    pub fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(message));
    }

    /// Shows a folder in the tree and hides the welcome page.
    async fn show_root(&self, root: PathBuf) {
        self.stack.set_visible_child_name("main");
        let name = self.session.borrow().root_name();
        self.folder.set_label(name.as_deref().unwrap_or_default());
        self.update_selection();
        if let Err(err) = self.tree.set_root(root).await {
            self.toast(&err);
        }
    }

    /// Title and tree highlight follow the selected tab.
    fn update_selection(&self) {
        let path = self.selected_path();
        self.tree.mark_active(path.clone());
        let title = match &path {
            Some(path) => match help_name(path) {
                Some(name) => help::page(name).map_or_else(
                    || window_title(None),
                    |page| window_title(Some(page.title.as_ref())),
                ),
                None => window_title(path.file_name()),
            },
            None => window_title(
                self.session
                    .borrow()
                    .root
                    .as_deref()
                    .map(|root| root.file_name().unwrap_or(root.as_os_str())),
            ),
        };
        self.win.set_title(Some(&title));
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.tabs
            .selected_page()
            .and_then(|page| page.tooltip())
            .map(|path| PathBuf::from(path.as_str()))
    }

    pub fn close_tab(&self) {
        if let Some(page) = self.tabs.selected_page() {
            self.tabs.close_page(&page);
        }
    }

    fn close_all_tabs(&self) {
        while let Some(page) = self.tabs.selected_page() {
            self.tabs.close_page(&page);
        }
    }

    /// Asks for a file, or a folder, and opens it.
    pub fn pick(self: &Rc<Self>, folder: bool) {
        if self.picking.replace(true) {
            return;
        }
        let this = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::new();
            if let Some(root) = this.session.borrow().root.as_deref() {
                dialog.set_initial_folder(Some(&gio::File::for_path(root)));
            }
            let picked = if folder {
                dialog.set_title("Open Folder");
                dialog.select_folder_future(Some(&this.win)).await
            } else {
                dialog.set_title("Open File");
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Markdown"));
                for extension in MARKDOWN_EXTENSIONS {
                    filter.add_suffix(extension);
                }
                let filters = gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                dialog.set_default_filter(Some(&filter));
                dialog.open_future(Some(&this.win)).await
            };
            this.picking.set(false);
            // Cancelling the dialog is an error too.
            if let Some(path) = picked.ok().and_then(|file| file.path()) {
                this.open_path(&path).await;
            }
        });
    }

    /// Opens a file or a folder chosen by the user. A file inside the folder
    /// already opened keeps it; any other file opens its own folder.
    pub async fn open_path(self: &Rc<Self>, path: &Path) {
        let opened = self.session.borrow_mut().open(&path.to_string_lossy());
        let opened = match opened {
            Ok(opened) => opened,
            Err(err) => return self.toast(&err),
        };
        if opened.root_changed {
            self.close_all_tabs();
            self.show_root(opened.root).await;
        }
        if let Some(file) = opened.file {
            self.open_document(file.clone());
            self.tree.reveal(&file).await;
        }
    }

    /// Opens a file in a tab after the active one, or brings it to the front.
    pub fn open_document(self: &Rc<Self>, path: PathBuf) {
        self.open_at(path, String::new(), false);
    }

    /// Opens a file and scrolls to `fragment`; `reveal` also shows the file in
    /// the tree (links do, the tree already shows what it opens).
    fn open_at(self: &Rc<Self>, path: PathBuf, fragment: String, reveal: bool) {
        let this = self.clone();
        glib::spawn_future_local(async move {
            match this.load_document(path, &fragment).await {
                Ok(resolved) if reveal => this.tree.reveal(&resolved).await,
                Ok(_) => {}
                Err(err) => this.toast(&err),
            }
        });
    }

    /// Where a link of a document leads, among the ones documents do not
    /// handle themselves.
    fn follow(self: &Rc<Self>, link: Link) {
        match link {
            Link::Help(name) => self.open_help(&name),
            Link::Local { path, fragment } => self.open_at(path, fragment, true),
            _ => {}
        }
    }

    /// The callback documents use to reach the window. Help pages cannot open
    /// files: their relative links are inert.
    fn linker(self: &Rc<Self>, files: bool) -> Rc<dyn Fn(Link)> {
        let this = Rc::downgrade(self);
        Rc::new(move |link| {
            let Some(this) = this.upgrade() else { return };
            if files || matches!(link, Link::Help(_)) {
                this.follow(link);
            }
        })
    }

    /// Shows a help page of salak-core in a tab.
    pub fn open_help(self: &Rc<Self>, name: &str) {
        let Some(page) = help::page(name) else {
            return self.toast(&format!("no help page named {name}"));
        };
        // Without folder the welcome page is showing, which has no tabs.
        self.stack.set_visible_child_name("main");
        let key = format!("{HELP_PREFIX}{name}");
        if self.select_existing(Path::new(&key)).is_some() {
            return;
        }
        let document = Document::new(
            page.markdown,
            Path::new(""),
            Path::new(""),
            self.linker(false),
        );
        self.add_tab(&document, page.title, &key);
    }

    /// Opens a file and returns its canonical path.
    async fn load_document(
        self: &Rc<Self>,
        path: PathBuf,
        fragment: &str,
    ) -> Result<PathBuf, String> {
        let Some(root) = self.session.borrow().root.clone() else {
            return Err("no folder is opened".into());
        };
        let resolved = {
            let root = root.clone();
            let path = path.to_string_lossy().into_owned();
            gio::spawn_blocking(move || files::resolve_in_root(&root, &path))
                .await
                .map_err(|_| "opening the file failed".to_string())??
        };
        // The canonical path merges the different ways to name a file.
        if self.reuse(&resolved, fragment) {
            return Ok(resolved);
        }
        let (bytes, _) = gio::File::for_path(&resolved)
            .load_contents_future()
            .await
            .map_err(|err| format!("{}: {}", resolved.display(), err.message()))?;
        // Another load of the same file may have finished meanwhile.
        if self.reuse(&resolved, fragment) {
            return Ok(resolved);
        }

        let base = resolved.parent().unwrap_or(&root);
        let document = Document::new(
            &String::from_utf8_lossy(&bytes),
            base,
            &root,
            self.linker(true),
        );
        let title = resolved.file_name().unwrap_or_default().to_string_lossy();
        self.add_tab(&document, &title, &resolved.to_string_lossy());
        scroll_to_fragment(&document, fragment);
        Ok(resolved)
    }

    /// Adds a tab after the active one and selects it.
    fn add_tab(&self, document: &Document, title: &str, key: &str) {
        let page = match self.tabs.selected_page() {
            Some(selected) => self
                .tabs
                .insert(&document.widget, self.tabs.page_position(&selected) + 1),
            None => self.tabs.append(&document.widget),
        };
        page.set_title(title);
        // The tooltip doubles as the identity of the tab: its canonical path,
        // or `help:<name>`.
        page.set_tooltip(key);
        self.tabs.set_selected_page(&page);
    }

    /// Brings the tab of `path` to the front, if there is one, and returns
    /// its content.
    fn select_existing(&self, path: &Path) -> Option<gtk::Widget> {
        let path = path.to_string_lossy();
        let page = (0..self.tabs.n_pages())
            .map(|position| self.tabs.nth_page(position))
            .find(|page| page.tooltip().as_deref() == Some(&*path))?;
        self.tabs.set_selected_page(&page);
        Some(page.child())
    }

    /// Like `select_existing`, scrolling to `fragment`.
    fn reuse(&self, path: &Path, fragment: &str) -> bool {
        let Some(child) = self.select_existing(path) else {
            return false;
        };
        if let Some(document) = Document::of(&child) {
            scroll_to_fragment(&document, fragment);
        }
        true
    }
}

const HELP_PREFIX: &str = "help:";

/// The name of the help page a tab shows, if it shows one.
fn help_name(key: &Path) -> Option<&str> {
    key.to_str()?.strip_prefix(HELP_PREFIX)
}

fn scroll_to_fragment(document: &Document, fragment: &str) {
    if !fragment.is_empty() {
        document.scroll_to(fragment);
    }
}

/// Style of the tree rows.
fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(".tree-active { font-weight: bold; }");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
