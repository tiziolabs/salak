"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tree = document.getElementById("tree");
const content = document.getElementById("content");
const banner = document.getElementById("banner");
const welcome = document.getElementById("welcome");

// The document is rendered in a shadow root: its style sheet cannot leak
// into the interface and vice versa.
const shadow = document.getElementById("doc").attachShadow({ mode: "open" });
const style = document.createElement("link");
style.rel = "stylesheet";
style.href = "markdown.css";
// Comes after the default style sheet, so it wins at equal specificity.
const userTheme = document.createElement("style");
const article = document.createElement("article");
article.className = "markdown-body";
shadow.append(style, userTheme, article);

let session = null;
let current = null;
let selected = null;

// ---------------------------------------------------------------- tree

function makeNode(entry, depth) {
  const li = document.createElement("li");
  li.dataset.path = entry.path;
  li.dataset.depth = depth;
  li.setAttribute("role", "treeitem");
  if (entry.is_dir) li.classList.add("dir");

  const row = document.createElement("div");
  row.className = "row";
  row.style.setProperty("--depth", depth);
  const twisty = document.createElement("span");
  twisty.className = "twisty";
  const name = document.createElement("span");
  name.textContent = entry.name;
  row.append(twisty, name);
  li.append(row);

  if (entry.is_dir) li.append(document.createElement("ul"));
  return li;
}

async function fillList(ul, dirPath, depth) {
  const entries = await invoke("list_dir", { path: dirPath });
  ul.replaceChildren(...entries.map((entry) => makeNode(entry, depth)));
  if (current) markActive(current);
}

// Children are re-read on every expansion, so the tree picks up new files.
async function expand(li) {
  await fillList(li.querySelector(":scope > ul"), li.dataset.path, Number(li.dataset.depth) + 1);
  li.classList.add("expanded");
}

function collapse(li) {
  li.classList.remove("expanded");
}

function findNode(path) {
  return tree.querySelector(`li[data-path="${CSS.escape(path)}"]`);
}

// Expands every ancestor folder of `path` so that it becomes visible.
async function reveal(path) {
  if (!session.root || !path.startsWith(session.root + session.separator)) return;
  const parts = path.slice(session.root.length + 1).split(session.separator);
  let prefix = session.root;
  for (const part of parts.slice(0, -1)) {
    prefix += session.separator + part;
    const li = findNode(prefix);
    if (!li) return;
    if (!li.classList.contains("expanded")) await expand(li);
  }
  const li = findNode(path);
  if (li) select(li.firstElementChild);
}

function markActive(path) {
  tree.querySelector(".row.active")?.classList.remove("active");
  findNode(path)?.firstElementChild.classList.add("active");
}

function select(row) {
  selected?.classList.remove("selected");
  selected = row;
  row.classList.add("selected");
  row.scrollIntoView({ block: "nearest" });
}

function visibleRows() {
  return [...tree.querySelectorAll(".row")].filter((row) => row.offsetParent !== null);
}

function moveSelection(delta) {
  const rows = visibleRows();
  if (rows.length === 0) return;
  const index = rows.indexOf(selected);
  const next = index < 0 ? 0 : Math.min(Math.max(index + delta, 0), rows.length - 1);
  select(rows[next]);
}

async function activateRow(row) {
  const li = row.parentElement;
  if (li.classList.contains("dir")) {
    if (li.classList.contains("expanded")) collapse(li);
    else await expand(li);
  } else {
    await openTab(li.dataset.path);
  }
}

tree.addEventListener("click", (event) => {
  const row = event.target.closest(".row");
  if (!row) return;
  select(row);
  activateRow(row);
});

tree.addEventListener("keydown", async (event) => {
  if (event.ctrlKey || event.altKey || event.metaKey) return;
  const li = selected?.parentElement;
  const isDir = li?.classList.contains("dir");
  const expanded = li?.classList.contains("expanded");

  switch (event.key) {
    case "ArrowDown":
    case "j":
      moveSelection(1);
      break;
    case "ArrowUp":
    case "k":
      moveSelection(-1);
      break;
    case "Home":
    case "g":
      moveSelection(-Infinity);
      break;
    case "End":
    case "G":
      moveSelection(Infinity);
      break;
    case "ArrowRight":
    case "l":
      if (isDir && !expanded) await expand(li);
      else if (isDir) moveSelection(1);
      break;
    case "ArrowLeft":
    case "h":
      if (isDir && expanded) collapse(li);
      else if (li && li.parentElement !== tree) select(li.parentElement.parentElement.firstElementChild);
      break;
    case "Enter":
    case "o":
      if (selected) await activateRow(selected);
      break;
    default:
      return;
  }
  event.preventDefault();
});

// ---------------------------------------------------------------- document

const MARKDOWN_FILE = /\.(md|markdown|mdown|mkdn?)$/i;

function showMessage(text) {
  const p = document.createElement("p");
  p.className = "placeholder";
  p.textContent = text;
  article.replaceChildren(p);
  find();
}

function scrollToId(id) {
  article.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView();
}

function showDocument(doc) {
  current = doc.path;
  banner.hidden = true;
  // Parsing in a <template> does not start image loads before insertion.
  const template = document.createElement("template");
  template.innerHTML = doc.html;
  article.replaceChildren(template.content);
  markActive(current);
  find();
}

// Help pages are embedded in the binary and use `help:<name>` as path.
function load(path) {
  return path.startsWith("help:")
    ? invoke("open_help", { name: path.slice(5) })
    : invoke("open_file", { path });
}

// ---------------------------------------------------------------- find

const findBar = document.getElementById("find");
const findInput = document.getElementById("find-input");
const findCount = document.getElementById("find-count");
const findButtons = [document.getElementById("find-prev"), document.getElementById("find-next")];

// The matches of the query in the document, as ranges, and the current one.
let matches = [];
let currentMatch = 0;
// Highlights need no change to the document; WebView2 supports them.
const matchHighlight = window.Highlight ? new Highlight() : null;
const currentHighlight = window.Highlight ? new Highlight() : null;
if (matchHighlight) {
  CSS.highlights.set("find-match", matchHighlight);
  CSS.highlights.set("find-current", currentHighlight);
}

function clearFind() {
  matches = [];
  matchHighlight?.clear();
  currentHighlight?.clear();
  findCount.textContent = "";
  findInput.classList.remove("none");
  for (const button of findButtons) button.disabled = true;
}

// Looks for the query again in the document, from its first occurrence.
function find() {
  clearFind();
  const query = findInput.value.toLowerCase();
  if (findBar.hidden || !query) return;
  const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node && matches.length < 10000; node = walker.nextNode()) {
    const text = node.data.toLowerCase();
    for (let at = text.indexOf(query); at >= 0; at = text.indexOf(query, at + query.length)) {
      const range = new Range();
      range.setStart(node, at);
      range.setEnd(node, at + query.length);
      matches.push(range);
    }
  }
  matchHighlight?.add(...matches);
  findInput.classList.toggle("none", matches.length === 0);
  for (const button of findButtons) button.disabled = matches.length === 0;
  if (matches.length === 0) findCount.textContent = "No results";
  else showMatch(0);
}

function showMatch(index) {
  currentMatch = index;
  currentHighlight?.clear();
  currentHighlight?.add(matches[index]);
  findCount.textContent = `${index + 1} of ${matches.length}`;
  const rect = matches[index].getBoundingClientRect();
  const view = content.getBoundingClientRect();
  if (rect.top < view.top + view.height * 0.1 || rect.bottom > view.top + view.height * 0.9) {
    content.scrollBy(0, rect.top - view.top - view.height * 0.3);
  }
}

function stepMatch(delta) {
  if (matches.length) showMatch((currentMatch + delta + matches.length) % matches.length);
}

function openFind() {
  if (!active) return;
  findBar.hidden = false;
  findInput.focus();
  findInput.select();
  find();
}

function closeFind() {
  findBar.hidden = true;
  clearFind();
  content.focus();
}

findInput.addEventListener("input", find);
findInput.addEventListener("keydown", (event) => {
  if (event.key === "Escape") closeFind();
  else if (event.key === "Enter") stepMatch(event.shiftKey ? -1 : 1);
  else return;
  event.preventDefault();
});
findButtons[0].addEventListener("click", () => stepMatch(-1));
findButtons[1].addEventListener("click", () => stepMatch(1));
document.getElementById("find-close").addEventListener("click", closeFind);
// The form never submits: Enter is handled above.
findBar.addEventListener("submit", (event) => event.preventDefault());

// ---------------------------------------------------------------- tabs

const tabBar = document.getElementById("tabs");
const tabMenu = document.getElementById("tab-menu");

// Open documents, in the order of the tab bar: { path, title, scroll }.
// Only the active one is rendered and watched; the others are read again
// when activated, so they never show a stale version.
let tabs = [];
let active = null;
// Counts loads, so that a slow one cannot overwrite a newer one.
let loads = 0;

function renderTabs() {
  tabBar.replaceChildren(
    ...tabs.map((tab, index) => {
      const element = document.createElement("div");
      element.className = "tab";
      element.dataset.index = index;
      element.setAttribute("role", "tab");
      element.setAttribute("aria-selected", tab === active);
      element.classList.toggle("active", tab === active);
      element.title = tab.path.startsWith("help:") ? tab.title : tab.path;
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = tab.title;
      const close = document.createElement("button");
      close.className = "close";
      close.tabIndex = -1;
      close.title = "Close (Ctrl+W)";
      close.textContent = "×";
      element.append(label, close);
      return element;
    }),
  );
  tabBar.querySelector(".active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

async function activate(tab, fragment = "") {
  if (active && active !== tab) active.scroll = content.scrollTop;
  active = tab;
  renderTabs();
  const ticket = ++loads;
  let doc;
  try {
    doc = await load(tab.path);
  } catch (err) {
    if (ticket !== loads) return;
    current = tab.path;
    banner.hidden = true;
    showMessage(String(err));
    return;
  }
  if (ticket !== loads) return;

  // The canonical path may reveal that this file is already open.
  const twin = tabs.find((other) => other !== tab && other.path === doc.path);
  if (twin) {
    tabs.splice(tabs.indexOf(tab), 1);
    active = tab = twin;
  }
  tab.path = doc.path;
  tab.title = doc.title;
  renderTabs();
  showDocument(doc);
  content.scrollTop = fragment ? 0 : tab.scroll;
  if (fragment) scrollToId(decodeURIComponent(fragment));
}

// Reads the active document again, keeping its scroll position.
async function reload() {
  if (!active) return;
  active.scroll = content.scrollTop;
  await activate(active);
}

// Opens a document in a new tab, right after the active one. A document
// already open is only brought to the front.
async function openTab(path, fragment = "") {
  let tab = tabs.find((other) => other.path === path);
  if (!tab) {
    const title = path.startsWith("help:") ? path.slice(5) : path.split(session.separator).pop();
    tab = { path, title, scroll: 0 };
    tabs.splice(active ? tabs.indexOf(active) + 1 : tabs.length, 0, tab);
  }
  welcome.hidden = true;
  document.body.classList.remove("welcome");
  if (tab === active) {
    if (fragment) scrollToId(decodeURIComponent(fragment));
    return;
  }
  await activate(tab, fragment);
}

function closeTabs(closing) {
  if (closing.length === 0) return;
  const before = tabs;
  tabs = tabs.filter((tab) => !closing.includes(tab));
  if (!closing.includes(active)) {
    renderTabs();
    return;
  }
  // As in browsers: the tab on the right of the active one, else on its left.
  const index = before.indexOf(active);
  const next =
    before.slice(index + 1).find((tab) => tabs.includes(tab)) ??
    before.slice(0, index).reverse().find((tab) => tabs.includes(tab));
  if (next) {
    activate(next);
    return;
  }
  active = null;
  current = null;
  loads++;
  findBar.hidden = true;
  renderTabs();
  banner.hidden = true;
  markActive(null);
  invoke("close_document").catch(console.error);
  // Without a folder, the welcome page comes back.
  if (session.root) showMessage("Select a Markdown file in the tree.");
  else start(session);
}

function tabAt(element) {
  const tab = element?.closest(".tab");
  return tab ? tabs[Number(tab.dataset.index)] : null;
}

tabBar.addEventListener("click", (event) => {
  const tab = tabAt(event.target);
  if (!tab) return;
  if (event.target.closest(".close")) closeTabs([tab]);
  else if (tab !== active) activate(tab);
});

// Middle click closes, as in browsers.
tabBar.addEventListener("mousedown", (event) => {
  if (event.button === 1) event.preventDefault();
});
tabBar.addEventListener("auxclick", (event) => {
  const tab = tabAt(event.target);
  if (tab && event.button === 1) closeTabs([tab]);
});

function cycleTabs(delta) {
  if (tabs.length < 2) return;
  activate(tabs[(tabs.indexOf(active) + delta + tabs.length) % tabs.length]);
}

// ---------------------------------------------------------------- tab menu

let menuTab = null;

function tabsToClose(action, tab) {
  const index = tabs.indexOf(tab);
  switch (action) {
    case "close":
      return [tab];
    case "close-others":
      return tabs.filter((other) => other !== tab);
    case "close-right":
      return tabs.slice(index + 1);
    case "close-left":
      return tabs.slice(0, index);
  }
  return [];
}

function showTabMenu(tab, x, y) {
  menuTab = tab;
  for (const button of tabMenu.querySelectorAll("button")) {
    button.disabled = tabsToClose(button.dataset.action, tab).length === 0;
  }
  tabMenu.hidden = false;
  // Keeps the menu inside the window.
  const { width, height } = tabMenu.getBoundingClientRect();
  tabMenu.style.left = `${Math.min(x, window.innerWidth - width - 4)}px`;
  tabMenu.style.top = `${Math.min(y, window.innerHeight - height - 4)}px`;
  tabMenu.querySelector("button:enabled").focus();
}

// Focus only goes back to the document when the menu is left with the
// keyboard: a click elsewhere puts it where it was clicked.
function hideTabMenu(refocus = false) {
  if (tabMenu.hidden) return;
  tabMenu.hidden = true;
  menuTab = null;
  if (refocus) content.focus();
}

tabBar.addEventListener("contextmenu", (event) => {
  const tab = tabAt(event.target);
  if (!tab) return;
  event.preventDefault();
  showTabMenu(tab, event.clientX, event.clientY);
});

tabMenu.addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button || !menuTab) return;
  const closing = tabsToClose(button.dataset.action, menuTab);
  hideTabMenu(true);
  closeTabs(closing);
});

document.addEventListener("mousedown", (event) => {
  if (!tabMenu.contains(event.target)) hideTabMenu();
});
window.addEventListener("blur", () => hideTabMenu());

function moveInTabMenu(delta) {
  const buttons = [...tabMenu.querySelectorAll("button:enabled")];
  const index = buttons.indexOf(document.activeElement);
  buttons[(index + delta + buttons.length) % buttons.length].focus();
}

// ---------------------------------------------------------------- changes

async function loadTheme() {
  try {
    userTheme.textContent = (await invoke("theme_css")) ?? "";
  } catch (err) {
    console.error(err);
  }
}

// Applied right away: no banner, so that a theme can be tuned live.
listen("theme-changed", loadTheme);

function showBanner() {
  const text = document.createElement("span");
  text.textContent = "This file has changed on disk.";
  const reloadButton = document.createElement("button");
  reloadButton.textContent = "Reload (r)";
  reloadButton.addEventListener("click", reload);
  const dismissButton = document.createElement("button");
  dismissButton.textContent = "Dismiss (Esc)";
  dismissButton.addEventListener("click", () => (banner.hidden = true));
  banner.replaceChildren(text, reloadButton, dismissButton);
  banner.hidden = false;
}

listen("file-changed", (event) => {
  if (event.payload === current) showBanner();
});

shadow.addEventListener("click", async (event) => {
  const link = event.target.closest("a[href]");
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute("href");

  if (href.startsWith("#")) {
    scrollToId(decodeURIComponent(href.slice(1)));
  } else if (href.startsWith("help:")) {
    await openTab(href);
  } else if (href.startsWith("salak:")) {
    const hash = href.indexOf("#");
    const path = decodeURIComponent(href.slice(6, hash < 0 ? undefined : hash));
    if (!MARKDOWN_FILE.test(path)) return;
    await openTab(path, hash < 0 ? "" : href.slice(hash + 1));
    await reveal(current);
  } else {
    invoke("open_url", { url: href }).catch(console.error);
  }
});

// ---------------------------------------------------------------- opening

// Shows a folder, and opens a file in it if any. Without a folder, shows
// the welcome page.
async function start(next) {
  session = next;
  current = null;
  selected = null;
  tabs = [];
  active = null;
  loads++;
  renderTabs();
  banner.hidden = true;
  welcome.hidden = session.root !== null;
  document.body.classList.toggle("welcome", session.root === null);
  document.body.classList.toggle("no-root", session.root === null);
  if (!session.root) {
    article.replaceChildren();
    tree.replaceChildren();
    document.getElementById("open-file").focus();
    await showRecent();
    return;
  }
  document.getElementById("root-name").textContent = session.root_name;
  tree.dataset.path = session.root;
  await fillList(tree, session.root, 0);

  if (session.initial) {
    await reveal(session.initial);
    await openTab(session.initial);
    content.focus();
  } else {
    showMessage("Select a Markdown file in the tree.");
    moveSelection(0);
    tree.focus();
  }
}

// Lists the files and folders opened lately on the welcome page.
async function showRecent() {
  const section = document.getElementById("recent");
  let entries = [];
  try {
    entries = await invoke("recent_paths");
  } catch (err) {
    console.error(err);
  }
  section.hidden = entries.length === 0;
  document.getElementById("recent-list").replaceChildren(
    ...entries.map((entry) => {
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.title = entry.path;
      const name = document.createElement("span");
      name.className = "recent-name";
      name.textContent = (entry.is_dir ? "\u{1F4C1} " : "\u{1F4C4} ") + entry.name;
      const parent = document.createElement("span");
      parent.className = "recent-parent";
      parent.textContent = entry.parent;
      button.append(name, parent);
      button.addEventListener("click", () =>
        openPath(entry.path).catch((err) => showMessage(String(err))),
      );
      li.append(button);
      return li;
    }),
  );
}

async function openPath(path) {
  const next = await invoke("open_path", { path });
  // A file of the opened folder keeps the state of the tree.
  if (next.root === session.root && next.initial) {
    await openTab(next.initial);
    await reveal(current);
    content.focus();
  } else {
    await start(next);
  }
}

// Native file dialogs are modal: only one at a time.
let picking = false;

async function pick(folder) {
  if (picking) return;
  picking = true;
  try {
    const path = await invoke("pick", { folder });
    if (path) await openPath(path);
  } catch (err) {
    showMessage(String(err));
  } finally {
    picking = false;
  }
}

document.getElementById("open-file").addEventListener("click", () => pick(false));
document.getElementById("open-folder").addEventListener("click", () => pick(true));
document.getElementById("open-guide").addEventListener("click", (event) => {
  event.preventDefault();
  openHelp("user-guide");
});

function openHelp(name) {
  openTab(`help:${name}`).then(() => content.focus());
}

// ---------------------------------------------------------------- about

const aboutDialog = document.getElementById("about");

async function showAbout() {
  if (aboutDialog.open) return;
  try {
    const about = await invoke("about");
    document.getElementById("about-version").textContent = about.version;
    document.getElementById("about-license").textContent = about.license.replaceAll(" OR ", " or ");
    document.getElementById("about-author").textContent = about.author;
    const link = document.getElementById("about-repository");
    link.href = about.repository;
    link.textContent = about.repository.replace(/^https:\/\//, "");
    aboutDialog.showModal();
  } catch (err) {
    console.error(err);
  }
}

document.getElementById("about-repository").addEventListener("click", (event) => {
  event.preventDefault();
  invoke("open_url", { url: event.currentTarget.href }).catch(console.error);
});

// A click on the backdrop, outside of the dialog box, closes it.
aboutDialog.addEventListener("click", (event) => {
  if (event.target === aboutDialog) aboutDialog.close();
});

// Menu items, whose shortcuts (Ctrl+O, F1…) are handled by the menu itself.
listen("menu", (event) => {
  const id = event.payload;
  if (id === "open-file") pick(false);
  else if (id === "open-folder") pick(true);
  else if (id === "close-tab" && active) closeTabs([active]);
  else if (id === "find") openFind();
  else if (id === "about") showAbout();
  else if (id.startsWith("help:")) openHelp(id.slice(5));
});

// ---------------------------------------------------------------- global keys

// Vim-like scrolling of the document.
function scrollContent(key) {
  const page = content.clientHeight;
  const moves = {
    j: () => content.scrollBy(0, 48),
    k: () => content.scrollBy(0, -48),
    d: () => content.scrollBy(0, page / 2),
    u: () => content.scrollBy(0, -page / 2),
    g: () => content.scrollTo(0, 0),
    G: () => content.scrollTo(0, content.scrollHeight),
  };
  if (!(key in moves)) return false;
  moves[key]();
  return true;
}

// One button hides the sidebar from its header, the other one shows it
// again from the tab bar; only one is visible at a time.
const sidebarToggles = document.querySelectorAll(".sidebar-toggle");

function toggleSidebar() {
  const hidden = document.body.classList.toggle("no-sidebar");
  for (const button of sidebarToggles) button.setAttribute("aria-expanded", !hidden);
  if (hidden) content.focus();
  else tree.focus();
}

for (const button of sidebarToggles) button.addEventListener("click", toggleSidebar);

// Dragging the grip between the sidebar and the content resizes the sidebar.
const sidebar = document.getElementById("sidebar");
const grip = document.getElementById("sidebar-grip");

grip.addEventListener("pointerdown", (event) => {
  grip.setPointerCapture(event.pointerId);
  grip.classList.add("dragging");
  document.body.classList.add("dragging-sidebar");
});
grip.addEventListener("pointermove", (event) => {
  if (!grip.hasPointerCapture(event.pointerId)) return;
  const max = window.innerWidth * 0.6;
  sidebar.style.width = `${Math.min(Math.max(event.clientX, 140), max)}px`;
});
grip.addEventListener("pointerup", (event) => {
  grip.releasePointerCapture(event.pointerId);
  grip.classList.remove("dragging");
  document.body.classList.remove("dragging-sidebar");
});

document.addEventListener("keydown", (event) => {
  // The welcome page and the about dialog only have their buttons.
  if (!welcome.hidden || aboutDialog.open) return;
  const ctrl = event.ctrlKey || event.metaKey;
  // The keys of the search field are its own.
  if (event.target === findInput && !(ctrl && event.key === "f")) return;
  if (!tabMenu.hidden) {
    if (event.key === "Escape") hideTabMenu(true);
    else if (event.key === "ArrowDown") moveInTabMenu(1);
    else if (event.key === "ArrowUp") moveInTabMenu(-1);
    // Enter and Space activate the focused item.
    else return;
  } else if (ctrl && (event.key === "PageDown" || event.key === "PageUp")) {
    cycleTabs(event.key === "PageDown" ? 1 : -1);
  } else if (event.key === "Tab" && !ctrl && !event.altKey) {
    if (document.activeElement === tree || document.body.classList.contains("no-sidebar")) content.focus();
    else tree.focus();
  } else if ((event.key === "r" && !event.altKey) || event.key === "F5") {
    reload();
  } else if (event.key === "Escape" && !banner.hidden) {
    banner.hidden = true;
  } else if ((ctrl && event.key === "f") || (event.key === "/" && !ctrl && !event.altKey)) {
    openFind();
  } else if (event.key === "b" && !event.altKey) {
    toggleSidebar();
  } else if (content.contains(document.activeElement) && !ctrl && !event.altKey && scrollContent(event.key)) {
    // Handled.
  } else {
    return;
  }
  event.preventDefault();
});

// ---------------------------------------------------------------- startup

(async () => {
  await loadTheme();
  await start(await invoke("session"));
})().catch((err) => showMessage(String(err)));
