//! The lazy file tree of the opened folder.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::glib::{self, BoxedAnyObject, SignalHandlerId};
use gtk::{gio, prelude::*};
use salak_core::files::{self, Entry};

/// Rows currently shown by the list view, with their "expanded" handler.
type Bound = HashMap<gtk::ListItem, (gtk::TreeListRow, SignalHandlerId)>;

type OpenCallback = Box<dyn Fn(PathBuf)>;

pub struct Tree {
    root: RefCell<Option<PathBuf>>,
    /// Document shown in bold.
    active: RefCell<Option<PathBuf>>,
    top: gio::ListStore,
    model: gtk::TreeListModel,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    bound: Rc<RefCell<Bound>>,
    on_open: RefCell<Option<OpenCallback>>,
}

/// Lists a folder off the UI thread.
async fn list(root: PathBuf, dir: PathBuf) -> Result<Vec<Entry>, String> {
    gio::spawn_blocking(move || {
        let dir = files::resolve_in_root(&root, &dir.to_string_lossy())?;
        files::list_dir(&dir)
    })
    .await
    .map_err(|_| "listing the folder failed".to_string())?
}

fn items(entries: Vec<Entry>) -> Vec<BoxedAnyObject> {
    entries.into_iter().map(BoxedAnyObject::new).collect()
}

fn entry_path(row: &gtk::TreeListRow) -> Option<PathBuf> {
    let item = row.item().and_downcast::<BoxedAnyObject>()?;
    let entry = item.borrow::<Entry>();
    Some(entry.path.clone())
}

impl Tree {
    pub fn new() -> Rc<Tree> {
        let top = gio::ListStore::new::<BoxedAnyObject>();
        // Folders get an empty child model, filled when they are expanded.
        let model = gtk::TreeListModel::new(top.clone(), false, false, |item| {
            let item = item.downcast_ref::<BoxedAnyObject>()?;
            let is_dir = item.borrow::<Entry>().is_dir;
            is_dir.then(|| gio::ListStore::new::<BoxedAnyObject>().upcast())
        });
        let selection = gtk::SingleSelection::builder()
            .model(&model)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let list = gtk::ListView::builder()
            .model(&selection)
            .single_click_activate(true)
            .css_classes(["navigation-sidebar"])
            .build();
        let tree = Rc::new(Tree {
            root: RefCell::default(),
            active: RefCell::default(),
            top,
            model,
            selection,
            list,
            bound: Rc::default(),
            on_open: RefCell::default(),
        });
        tree.setup_factory();
        tree.list.connect_activate(glib::clone!(
            #[weak]
            tree,
            move |_, position| tree.activate(position)
        ));
        tree
    }

    pub fn widget(&self) -> &gtk::ListView {
        &self.list
    }

    /// Called with the path of a file the user chose in the tree.
    pub fn connect_open(&self, callback: impl Fn(PathBuf) + 'static) {
        *self.on_open.borrow_mut() = Some(Box::new(callback));
    }

    fn setup_factory(self: &Rc<Self>) {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let icon = gtk::Image::new();
            let label = gtk::Label::builder()
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let row = gtk::Box::builder().spacing(6).build();
            row.append(&icon);
            row.append(&label);
            let expander = gtk::TreeExpander::builder().child(&row).build();
            item.set_child(Some(&expander));
        });
        factory.connect_bind(glib::clone!(
            #[weak(rename_to = tree)]
            self,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else {
                    return;
                };
                let handler = row.connect_expanded_notify(glib::clone!(
                    #[weak]
                    tree,
                    move |row| {
                        if row.is_expanded() {
                            glib::spawn_future_local(glib::clone!(
                                #[strong]
                                row,
                                async move {
                                    if let Err(err) = tree.load_children(&row).await {
                                        eprintln!("salak: {err}");
                                    }
                                }
                            ));
                        }
                    }
                ));
                tree.bound.borrow_mut().insert(item.clone(), (row, handler));
                tree.refresh(item);
            }
        ));
        factory.connect_unbind(glib::clone!(
            #[weak(rename_to = tree)]
            self,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                if let Some((row, handler)) = tree.bound.borrow_mut().remove(item) {
                    row.disconnect(handler);
                }
                if let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() {
                    expander.set_list_row(None);
                }
            }
        ));
        self.list.set_factory(Some(&factory));
    }

    /// Fills the widgets of a bound row from its entry.
    fn refresh(&self, item: &gtk::ListItem) {
        let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };
        expander.set_list_row(Some(&row));
        let Some(item) = row.item().and_downcast::<BoxedAnyObject>() else {
            return;
        };
        let entry = item.borrow::<Entry>();
        let content = expander.child().and_downcast::<gtk::Box>().unwrap();
        let icon = content.first_child().and_downcast::<gtk::Image>().unwrap();
        let label = content.last_child().and_downcast::<gtk::Label>().unwrap();
        icon.set_icon_name(Some(if entry.is_dir {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        }));
        label.set_label(&entry.name);
        let active = self.active.borrow().as_deref() == Some(entry.path.as_path());
        if active {
            label.add_css_class("tree-active");
        } else {
            label.remove_css_class("tree-active");
        }
    }

    fn activate(&self, position: u32) {
        let Some(row) = self.model.row(position) else {
            return;
        };
        let Some(path) = entry_path(&row) else { return };
        if row.is_expandable() {
            row.set_expanded(!row.is_expanded());
        } else if let Some(on_open) = &*self.on_open.borrow() {
            on_open(path);
        }
    }

    /// Lists a folder again every time it is expanded, so that files created
    /// in the meantime show up.
    async fn load_children(&self, row: &gtk::TreeListRow) -> Result<(), String> {
        let Some(store) = row.children().and_downcast::<gio::ListStore>() else {
            return Ok(());
        };
        let (Some(root), Some(dir)) = (self.root.borrow().clone(), entry_path(row)) else {
            return Ok(());
        };
        let entries = list(root, dir).await?;
        store.splice(0, store.n_items(), &items(entries));
        Ok(())
    }

    /// Shows another folder. An unreadable one leaves an empty tree.
    pub async fn set_root(&self, root: PathBuf) -> Result<(), String> {
        *self.root.borrow_mut() = Some(root.clone());
        *self.active.borrow_mut() = None;
        self.top.remove_all();
        let entries = list(root.clone(), root).await?;
        self.top.splice(0, 0, &items(entries));
        Ok(())
    }

    /// Expands the folders leading to `path`, then selects and scrolls to it.
    pub async fn reveal(&self, path: &Path) {
        let Some(root) = self.root.borrow().clone() else {
            return;
        };
        let Ok(relative) = path.strip_prefix(&root) else {
            return;
        };
        let mut folder = root;
        let mut folders = relative.components().collect::<Vec<_>>();
        folders.pop();
        for component in folders {
            folder.push(component);
            let Some(row) = self.find(&folder).and_then(|p| self.model.row(p)) else {
                return;
            };
            if !row.is_expanded() {
                row.set_expanded(true);
                if self.load_children(&row).await.is_err() {
                    return;
                }
            }
        }
        if let Some(position) = self.find(path) {
            self.selection.set_selected(position);
            self.list
                .scroll_to(position, gtk::ListScrollFlags::SELECT, None);
        }
    }

    /// Position of the row showing `path`, if it is currently listed.
    fn find(&self, path: &Path) -> Option<u32> {
        (0..self.model.n_items()).find(|&position| {
            self.model
                .row(position)
                .and_then(|row| entry_path(&row))
                .is_some_and(|candidate| candidate == path)
        })
    }

    pub fn focus(&self) {
        self.list.grab_focus();
    }

    /// Keyboard navigation. The arrows are handled here too, so that the list
    /// view does not move the selection a second time.
    pub fn key(&self, key: gtk::gdk::Key) -> bool {
        use gtk::gdk::Key;
        let count = self.model.n_items();
        let selected = self.selection.selected();
        let current = (selected != gtk::INVALID_LIST_POSITION).then_some(selected);
        let row = current.and_then(|position| self.model.row(position));
        let next = |position: u32| (position + 1).min(count.saturating_sub(1));
        match key {
            _ if count == 0 => return false,
            Key::Down | Key::j => self.select(current.map_or(0, next)),
            Key::Up | Key::k => self.select(current.map_or(count - 1, |p| p.saturating_sub(1))),
            Key::g | Key::Home => self.select(0),
            Key::G | Key::End => self.select(count - 1),
            Key::Right | Key::l => match (row, current) {
                (Some(row), Some(position)) if row.is_expandable() => {
                    if row.is_expanded() {
                        self.select(next(position));
                    } else {
                        row.set_expanded(true);
                    }
                }
                _ => {}
            },
            Key::Left | Key::h => match row {
                Some(row) if row.is_expanded() => row.set_expanded(false),
                Some(row) => {
                    if let Some(parent) = row.parent() {
                        self.select(parent.position());
                    }
                }
                None => {}
            },
            Key::Return | Key::KP_Enter | Key::o => {
                if let Some(position) = current {
                    self.activate(position);
                }
            }
            _ => return false,
        }
        true
    }

    fn select(&self, position: u32) {
        self.selection.set_selected(position);
        self.list
            .scroll_to(position, gtk::ListScrollFlags::FOCUS, None);
    }

    /// Shows the open document in bold.
    pub fn mark_active(&self, path: Option<PathBuf>) {
        *self.active.borrow_mut() = path;
        let bound: Vec<_> = self.bound.borrow().keys().cloned().collect();
        for item in bound {
            self.refresh(&item);
        }
    }
}
