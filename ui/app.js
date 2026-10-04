"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tree = document.getElementById("tree");
const content = document.getElementById("content");
const banner = document.getElementById("banner");

// The document is rendered in a shadow root: its style sheet cannot leak
// into the interface and vice versa.
const shadow = document.getElementById("doc").attachShadow({ mode: "open" });
const style = document.createElement("link");
style.rel = "stylesheet";
style.href = "markdown.css";
// Comes after the default style sheet, so it wins at equal specificity.
const userStyle = document.createElement("style");
const article = document.createElement("article");
article.className = "markdown-body";
shadow.append(style, userStyle, article);

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
  if (!path.startsWith(session.root + session.separator)) return;
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

async function activate(row) {
  const li = row.parentElement;
  if (li.classList.contains("dir")) {
    if (li.classList.contains("expanded")) collapse(li);
    else await expand(li);
  } else {
    await openFile(li.dataset.path);
  }
}

tree.addEventListener("click", (event) => {
  const row = event.target.closest(".row");
  if (!row) return;
  select(row);
  activate(row);
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
      if (selected) await activate(selected);
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
}

function scrollToId(id) {
  article.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView();
}

async function openFile(path, fragment = "") {
  try {
    const result = await invoke("open_file", { path });
    current = result.path;
    banner.hidden = true;
    // Parsing in a <template> does not start image loads before insertion.
    const template = document.createElement("template");
    template.innerHTML = result.html;
    article.replaceChildren(template.content);
    markActive(current);
    content.scrollTop = 0;
    if (fragment) scrollToId(decodeURIComponent(fragment));
  } catch (err) {
    showMessage(String(err));
  }
}

async function reload() {
  if (!current) return;
  const top = content.scrollTop;
  await openFile(current);
  content.scrollTop = top;
}

// ---------------------------------------------------------------- changes

async function loadUserStyle() {
  try {
    userStyle.textContent = (await invoke("user_style")) ?? "";
  } catch (err) {
    console.error(err);
  }
}

// Applied right away: no banner, so that a theme can be tuned live.
listen("style-changed", loadUserStyle);

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
  } else if (href.startsWith("salak:")) {
    const hash = href.indexOf("#");
    const path = decodeURIComponent(href.slice(6, hash < 0 ? undefined : hash));
    if (!MARKDOWN_FILE.test(path)) return;
    await openFile(path, hash < 0 ? "" : href.slice(hash + 1));
    await reveal(current);
  } else {
    invoke("open_url", { url: href }).catch(console.error);
  }
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

function toggleSidebar() {
  document.body.classList.toggle("no-sidebar");
  if (document.body.classList.contains("no-sidebar")) content.focus();
  else tree.focus();
}

document.addEventListener("keydown", (event) => {
  const ctrl = event.ctrlKey || event.metaKey;
  if (event.key === "Tab" && !ctrl && !event.altKey) {
    if (document.activeElement === tree || document.body.classList.contains("no-sidebar")) content.focus();
    else tree.focus();
  } else if ((event.key === "r" && !event.altKey) || event.key === "F5") {
    reload();
  } else if (event.key === "Escape" && !banner.hidden) {
    banner.hidden = true;
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
  session = await invoke("session");
  await loadUserStyle();
  document.getElementById("root-name").textContent = session.root_name;
  tree.dataset.path = session.root;
  await fillList(tree, session.root, 0);

  if (session.initial) {
    await reveal(session.initial);
    await openFile(session.initial);
    content.focus();
  } else {
    showMessage("Select a Markdown file in the tree.");
    moveSelection(0);
    tree.focus();
  }
})().catch((err) => showMessage(String(err)));
