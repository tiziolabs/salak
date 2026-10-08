//! The main window: sidebar tree, tabs and the welcome page.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use salak_core::files::{self, MARKDOWN_EXTENSIONS};
use salak_core::help;
use salak_core::markdown::Link;
use salak_core::recent;
use salak_core::session::{window_title, Session};

use crate::actions;
use crate::document::{self, Document};
use crate::keys;
use crate::monitor;
use crate::search::{self, Search};
use crate::theme::{self, Themer};
use crate::tree::Tree;

const MIN_SIDEBAR_WIDTH: f64 = 200.0;
const MAX_SIDEBAR_WIDTH: f64 = 420.0;

/// Wraps the sidebar with a thin handle on its trailing edge. AdwOverlaySplitView
/// has no user resizing, but it clamps its width between the minimum and the
/// maximum, so dragging pins both to the wanted width.
fn sidebar_with_grip(sidebar: &gtk::Box) -> gtk::Overlay {
    let grip = gtk::Box::builder()
        .halign(gtk::Align::End)
        .vexpand(true)
        .width_request(6)
        .cursor(&gtk::gdk::Cursor::from_name("col-resize", None).unwrap())
        .build();
    let overlay = gtk::Overlay::builder().child(sidebar).build();
    overlay.add_overlay(&grip);

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_update(glib::clone!(
        #[weak]
        overlay,
        #[weak]
        grip,
        move |drag, dx, _| {
            let Some(split) = overlay
                .ancestor(adw::OverlaySplitView::static_type())
                .and_downcast::<adw::OverlaySplitView>()
            else {
                return;
            };
            // The grip moves while the sidebar resizes, so its own offsets drift:
            // locate the pointer in the split view, whose origin stays put.
            let Some((start_x, start_y)) = drag.start_point() else {
                return;
            };
            let local = gtk::graphene::Point::new((start_x + dx) as f32, start_y as f32);
            let Some(point) = grip.compute_point(&split, &local) else {
                return;
            };
            let width = f64::from(point.x()).clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH * 2.0);
            split.set_max_sidebar_width(width);
            split.set_min_sidebar_width(width);
        }
    ));
    grip.add_controller(drag);
    overlay
}

pub struct Window {
    pub win: adw::ApplicationWindow,
    session: RefCell<Session>,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    pub split: adw::OverlaySplitView,
    folder: gtk::Label,
    pub tree: Rc<Tree>,
    pub tabs: adw::TabView,
    /// Tabs, or the hint shown when none is open.
    pages: gtk::Stack,
    /// Banner shown when the open file changed on disk.
    banner: adw::Banner,
    pub search: Rc<Search>,
    themer: Rc<Themer>,
    /// Watches the theme file, for live editing.
    theme_monitor: RefCell<Option<gio::FileMonitor>>,
    /// Watches the file of the selected tab.
    monitor: RefCell<Option<gio::FileMonitor>>,
    /// Hash of the content each tab was drawn from, by tab key.
    hashes: RefCell<HashMap<String, u64>>,
    /// Counts the reads, so that a slow one never overrides a newer one.
    loads: Cell<u64>,
    /// Only one file dialog at a time.
    picking: Cell<bool>,
}

/// Where the keyboard focus is, for the single-letter keys.
#[derive(PartialEq)]
pub enum Focus {
    Tree,
    Document,
    /// A text field, a menu or a dialog: keys are theirs.
    Other,
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
    let find = gio::Menu::new();
    find.append(Some("Find…"), Some("win.find"));
    menu.append_section(None, &find);
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

/// The welcome page, shown while no folder is open, its first button and the
/// list of recent files and folders.
fn welcome() -> (adw::StatusPage, gtk::Button, gtk::ListBox) {
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
    let recent = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let recent_title = gtk::Label::builder()
        .label("Recent")
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let recent_group = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        // Shown by `fill_recent` when there is something to list.
        .visible(false)
        .build();
    recent_group.append(&recent_title);
    recent_group.append(&recent);
    let clamp = adw::Clamp::builder()
        .maximum_size(460)
        .child(&recent_group)
        .build();
    content.append(&buttons);
    content.append(&guide);
    content.append(&clamp);
    let page = adw::StatusPage::builder()
        .icon_name(crate::APP_ID)
        .title("Salak")
        .description("Open a Markdown file, or a folder to browse its files.")
        .child(&content)
        .build();
    (page, open_file, recent)
}

impl Window {
    pub fn new(
        app: &adw::Application,
        session: Session,
        theme_path: Option<PathBuf>,
    ) -> Rc<Window> {
        install_css();
        document::install_css();
        let (theme, warning) = theme::read(theme_path.as_deref());
        let themer = Themer::new(theme);

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
        let sidebar = sidebar_with_grip(&sidebar);

        let tabs = adw::TabView::new();
        let tab_bar = adw::TabBar::builder().view(&tabs).autohide(false).build();
        let banner = adw::Banner::builder()
            .title("This file has changed on disk.")
            .button_label("Reload")
            .action_name("win.reload")
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let hint = adw::StatusPage::builder()
            .icon_name("text-x-generic-symbolic")
            .title("Select a Markdown file in the tree.")
            .build();
        let pages = gtk::Stack::builder().vexpand(true).build();
        pages.add_named(&tabs, Some("tabs"));
        pages.add_named(&hint, Some("hint"));
        content.append(&tab_bar);
        let search = Search::new();
        content.append(&banner);
        content.append(&search.bar);
        content.append(&pages);

        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .sidebar_width_fraction(0.25)
            .min_sidebar_width(MIN_SIDEBAR_WIDTH)
            .max_sidebar_width(MAX_SIDEBAR_WIDTH)
            .build();
        let stack = gtk::Stack::new();
        let (welcome, open_button, recent_list) = welcome();
        stack.add_named(&welcome, Some("welcome"));
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
        // Narrow windows overlay the sidebar instead of squeezing the text.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            600.0,
            adw::LengthUnit::Sp,
        ));
        narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
        narrow.add_setter(&split, "show-sidebar", Some(&false.to_value()));
        win.add_breakpoint(narrow);
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
            pages,
            banner,
            search,
            themer,
            theme_monitor: RefCell::default(),
            monitor: RefCell::default(),
            hashes: RefCell::default(),
            loads: Cell::new(0),
            picking: Cell::new(false),
        });
        this.banner.set_revealed(false);
        if let Some(warning) = warning {
            this.toast(&warning);
        }
        this.watch_theme(theme_path);
        actions::install(&this, app);
        keys::install(&this);
        search::install(&this);

        this.tree.connect_open(glib::clone!(
            #[weak]
            this,
            move |path| this.open_document(path)
        ));
        this.tabs.connect_selected_page_notify(glib::clone!(
            #[weak]
            this,
            move |_| this.selection_changed()
        ));
        this.tabs.connect_n_pages_notify(glib::clone!(
            #[weak]
            this,
            move |_| this.update_empty()
        ));
        this.tabs.connect_page_detached(glib::clone!(
            #[weak]
            this,
            move |_, page, _| {
                if let Some(key) = page.keyword() {
                    this.hashes.borrow_mut().remove(key.as_str());
                }
            }
        ));
        // Moves the focus to where the sidebar is, or away from where it was.
        this.split.connect_show_sidebar_notify(glib::clone!(
            #[weak]
            this,
            move |split| {
                if split.shows_sidebar() {
                    this.tree.focus();
                } else {
                    this.focus_document();
                }
            }
        ));

        // The root and the initial file come from the command line.
        let (root, initial) = {
            let session = this.session.borrow();
            (session.root.clone(), session.initial.clone())
        };
        if let Some(target) = initial.as_ref().or(root.as_ref()) {
            recent::record(target);
        }
        if let Some(root) = root {
            glib::spawn_future_local(glib::clone!(
                #[strong]
                this,
                async move {
                    this.show_root(root).await;
                    match initial {
                        Some(file) => match this.load_document(file, "").await {
                            Ok(resolved) => {
                                this.tree.reveal(&resolved).await;
                                this.focus_document();
                            }
                            Err(err) => this.toast(&err),
                        },
                        None => this.tree.focus(),
                    }
                }
            ));
        } else {
            this.fill_recent(&recent_list);
            this.stack.set_visible_child_name("welcome");
            GtkWindowExt::set_focus(&this.win, Some(&open_button));
        }
        this.win.present();
        this
    }

    /// Lists the recent files and folders on the welcome page.
    fn fill_recent(self: &Rc<Self>, list: &gtk::ListBox) {
        let entries = recent::load();
        if let Some(group) = list.parent() {
            group.set_visible(!entries.is_empty());
        }
        for path in entries {
            let name = path.file_name().unwrap_or(path.as_os_str());
            let location = path.parent().unwrap_or(&path);
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&name.to_string_lossy()))
                .subtitle(glib::markup_escape_text(&location.to_string_lossy()))
                .subtitle_lines(1)
                .tooltip_text(path.to_string_lossy())
                .activatable(true)
                .build();
            let icon = if path.is_dir() {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            };
            row.add_prefix(&gtk::Image::from_icon_name(icon));
            row.connect_activated(glib::clone!(
                #[strong(rename_to = this)]
                self,
                move |_| {
                    let this = this.clone();
                    let path = path.clone();
                    glib::spawn_future_local(async move { this.open_path(&path).await });
                }
            ));
            list.append(&row);
        }
    }

    /// Applies the theme again, with no banner, whenever its file is saved.
    /// Without a folder to watch, live editing starts with the next launch.
    fn watch_theme(self: &Rc<Self>, path: Option<PathBuf>) {
        let Some(path) = path else { return };
        let Some(target) = salak_core::theme::watch_target(&path) else {
            return;
        };
        let this = Rc::downgrade(self);
        *self.theme_monitor.borrow_mut() = monitor::watch(&target, move || {
            let Some(this) = this.upgrade() else { return };
            let (theme, warning) = theme::read(Some(&path));
            this.themer.set(theme);
            if let Some(warning) = warning {
                this.toast(&warning);
            }
        });
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

    /// The key of the selected tab: a canonical path or `help:<name>`.
    fn selected_path(&self) -> Option<PathBuf> {
        self.tabs
            .selected_page()
            .and_then(|page| page.keyword())
            .map(|key| PathBuf::from(key.as_str()))
    }

    /// The tab changed: the title, the watch and the content follow.
    fn selection_changed(self: &Rc<Self>) {
        self.update_selection();
        self.banner.set_revealed(false);
        self.research();
        if let Some(monitor) = self.monitor.take() {
            monitor.cancel();
        }
        let Some(path) = self
            .selected_path()
            .filter(|path| help_name(path).is_none())
        else {
            return;
        };
        // Only the selected tab is watched: the others are read again when
        // they are selected.
        let this = Rc::downgrade(self);
        *self.monitor.borrow_mut() = monitor::watch(&path, move || {
            if let Some(this) = this.upgrade() {
                this.banner.set_revealed(true);
            }
        });
        let this = self.clone();
        glib::spawn_future_local(async move { this.refresh(path, false).await });
    }

    /// Without tab, a hint (or the welcome page, if there is no folder).
    fn update_empty(&self) {
        let empty = self.tabs.n_pages() == 0;
        self.pages
            .set_visible_child_name(if empty { "hint" } else { "tabs" });
        self.set_find_enabled(!empty);
        if empty {
            self.search.close();
        }
        if empty && self.session.borrow().root.is_none() {
            self.stack.set_visible_child_name("welcome");
        }
    }

    /// Reads a tab's file again and draws it if it changed (or `force`),
    /// keeping the scroll position.
    async fn refresh(self: &Rc<Self>, path: PathBuf, force: bool) {
        let load = self.loads.get() + 1;
        self.loads.set(load);
        let result = self.read(&path).await;
        if self.loads.get() != load {
            return;
        }
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(err) if force => return self.toast(&err),
            // A file that vanished stays on screen until the tab is closed.
            Err(_) => return,
        };
        let key = path.to_string_lossy().into_owned();
        let hash = hash_of(&bytes);
        if !force && self.hashes.borrow().get(&key) == Some(&hash) {
            return;
        }
        let Some(root) = self.session.borrow().root.clone() else {
            return;
        };
        let Some(document) = self
            .find_page(&path)
            .and_then(|page| Document::of(&page.child()))
        else {
            return;
        };
        let base = path.parent().unwrap_or(&root);
        document.reload(
            &String::from_utf8_lossy(&bytes),
            base,
            &root,
            &self.themer,
            self.linker(true),
        );
        self.hashes.borrow_mut().insert(key, hash);
        self.research();
    }

    /// Reloads the document of the selected tab.
    pub fn reload(self: &Rc<Self>) {
        self.banner.set_revealed(false);
        if let Some(path) = self
            .selected_path()
            .filter(|path| help_name(path).is_none())
        {
            let this = self.clone();
            glib::spawn_future_local(async move { this.refresh(path, true).await });
        }
    }

    /// Opens the search bar on the selected document.
    pub fn find(&self) {
        if self.document().is_some() {
            self.search.open();
        }
    }

    /// Looks for the query again in the selected document.
    pub fn research(&self) {
        self.search.update(self.document().as_ref());
    }

    pub fn set_find_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .win
            .lookup_action("find")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    pub fn dismiss_banner(&self) {
        self.banner.set_revealed(false);
    }

    pub fn banner_revealed(&self) -> bool {
        self.banner.is_revealed()
    }

    pub fn toggle_sidebar(&self) {
        self.split.set_show_sidebar(!self.split.shows_sidebar());
    }

    pub fn sidebar_shown(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub fn focus_document(&self) {
        if let Some(document) = self.document() {
            document.focus();
        }
    }

    /// The document of the selected tab.
    pub fn document(&self) -> Option<Document> {
        Document::of(&self.tabs.selected_page()?.child())
    }

    pub fn focus_context(&self) -> Focus {
        let Some(focus) = GtkWindowExt::focus(&self.win) else {
            return Focus::Document;
        };
        if &focus == self.tree.widget() || focus.is_ancestor(self.tree.widget()) {
            Focus::Tree
        } else if [gtk::Text::static_type(), gtk::Popover::static_type()]
            .into_iter()
            .chain([adw::Dialog::static_type()])
            .any(|kind| focus.ancestor(kind).is_some())
        {
            Focus::Other
        } else {
            Focus::Document
        }
    }

    /// Help › About Salak.
    pub fn about(&self) {
        let about = salak_core::about::about();
        let dialog = adw::AboutDialog::builder()
            .application_name("Salak")
            .application_icon(crate::APP_ID)
            .version(about.version)
            .developer_name(about.author.as_str())
            .website(about.repository)
            .comments("A lightweight Markdown reader")
            .license_type(gtk::License::Custom)
            .license(about.license.replace(" OR ", " or "))
            .build();
        dialog.present(Some(&self.win));
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
        recent::record(opened.file.as_ref().unwrap_or(&opened.root));
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
            &self.themer,
            self.linker(false),
        );
        self.add_tab(&document, page.title, &key, page.title);
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
        let resolved = resolve(&root, &path).await?;
        // The canonical path merges the different ways to name a file.
        if self.reuse(&resolved, fragment) {
            return Ok(resolved);
        }
        let bytes = self.read(&resolved).await?;
        // Another load of the same file may have finished meanwhile.
        if self.reuse(&resolved, fragment) {
            return Ok(resolved);
        }

        let base = resolved.parent().unwrap_or(&root);
        let document = Document::new(
            &String::from_utf8_lossy(&bytes),
            base,
            &root,
            &self.themer,
            self.linker(true),
        );
        let title = resolved.file_name().unwrap_or_default().to_string_lossy();
        let key = resolved.to_string_lossy();
        self.hashes
            .borrow_mut()
            .insert(key.to_string(), hash_of(&bytes));
        self.add_tab(&document, &title, &key, &key);
        scroll_to_fragment(&document, fragment);
        Ok(resolved)
    }

    /// Reads a file of the opened folder.
    async fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        let Some(root) = self.session.borrow().root.clone() else {
            return Err("no folder is opened".into());
        };
        let resolved = resolve(&root, path).await?;
        let (bytes, _) = gio::File::for_path(&resolved)
            .load_contents_future()
            .await
            .map_err(|err| format!("{}: {}", resolved.display(), err.message()))?;
        Ok(bytes.to_vec())
    }

    /// Adds a tab after the active one and selects it. `key` identifies the
    /// document, `tooltip` is what hovering the tab shows.
    fn add_tab(&self, document: &Document, title: &str, key: &str, tooltip: &str) {
        let page = match self.tabs.selected_page() {
            Some(selected) => self
                .tabs
                .insert(&document.widget, self.tabs.page_position(&selected) + 1),
            None => self.tabs.append(&document.widget),
        };
        page.set_title(title);
        // The keyword only feeds the search of the tab overview, which Salak
        // does not use: it carries the identity of the tab instead (its
        // canonical path, or `help:<name>`).
        page.set_keyword(key);
        page.set_tooltip(tooltip);
        self.tabs.set_selected_page(&page);
    }

    fn find_page(&self, path: &Path) -> Option<adw::TabPage> {
        let path = path.to_string_lossy();
        (0..self.tabs.n_pages())
            .map(|position| self.tabs.nth_page(position))
            .find(|page| page.keyword().as_deref() == Some(&*path))
    }

    /// Brings the tab of `path` to the front, if there is one, and returns
    /// its content.
    fn select_existing(&self, path: &Path) -> Option<gtk::Widget> {
        let page = self.find_page(path)?;
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

/// Resolves a path inside the opened folder, off the UI thread.
async fn resolve(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root.to_path_buf();
    let path = path.to_string_lossy().into_owned();
    gio::spawn_blocking(move || files::resolve_in_root(&root, &path))
        .await
        .map_err(|_| "opening the file failed".to_string())?
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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
