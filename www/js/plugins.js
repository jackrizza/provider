/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
window.__pluginState = { currentId: null, editor: null };

function initMonaco(initial = '# Python\nprint("hello, monaco")\n') {
  // Configure Monaco loader (CDN)
  require.config({
    paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.0/min/vs" },
  });
  require(["vs/editor/editor.main"], function () {
    if (window.__pluginState.editor) return;
    window.__pluginState.editor = monaco.editor.create(
      document.getElementById("editor"),
      {
        value: initial,
        language: "python",
        theme: "vs-light", // switch to 'vs-dark' if you prefer
        automaticLayout: true,
      },
    );
  });
}

function setActivePlugin(id, name, entryPath, runtime) {
  window.__pluginState.currentId = id;

  // Fill edit form
  document.getElementById("edit_id").value = id || "";
  document.getElementById("edit_name").value = name || "";
  document.getElementById("edit_entry_path").value = entryPath || "";
  document.getElementById("edit_runtime").value = runtime || "";

  // Load code (replace with a real fetch to your API for the entryPath file)
  if (window.__pluginState.editor) {
    window.__pluginState.editor.setValue(
      `# ${name} (${id})\n# entry: ${entryPath}\n`,
    );
  }

  // highlight selected in sidebar
  document
    .querySelectorAll("[data-plugin-item]")
    .forEach((el) => el.classList.remove("uk-active"));
  const li = document.querySelector(`[data-plugin-item="${CSS.escape(id)}"]`);
  if (li) li.classList.add("uk-active");
}

async function postForm(url, formEl) {
  const fd = new FormData(formEl);
  const r = await fetch(url, { method: "POST", body: fd });
  if (!r.ok) throw new Error(await r.text());
}

async function onCreate(e) {
  e.preventDefault();
  await postForm("/plug_ins/new", e.target);
  location.reload();
}

async function onUpdate(e) {
  e.preventDefault();
  const id = document.getElementById("edit_id").value;
  if (!id) {
    alert("Select a plugin first");
    return;
  }
  await postForm(`/plug_ins/${encodeURIComponent(id)}/update`, e.target);
  location.reload();
}
function toUrlEncoded(from) {
  const p = new URLSearchParams();
  if (from instanceof HTMLFormElement) {
    new FormData(from).forEach((v, k) => {
      // Files can't be urlencoded; if you add file inputs later, switch server to Multipart.
      p.append(k, typeof v === "string" ? v : String(v));
    });
  } else if (from && typeof from === "object") {
    Object.entries(from).forEach(([k, v]) => p.append(k, v ?? ""));
  }
  return p.toString();
}

async function postForm(url, formEl) {
  const body = toUrlEncoded(formEl);
  const r = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
    },
    body,
  });
  if (!r.ok) throw new Error(await r.text());
}

async function onCreate(e) {
  e.preventDefault();
  await postForm("/plug_ins/new", e.target);
  location.reload();
}

async function onUpdate(e) {
  e.preventDefault();
  const id = document.getElementById("edit_id").value;
  if (!id) {
    alert("Select a plugin first");
    return;
  }
  await postForm(`/plug_ins/${encodeURIComponent(id)}/update`, e.target);
  location.reload();
}

async function onDelete(id) {
  if (!confirm("Delete this plugin?")) return;
  const body = toUrlEncoded({ id });
  const r = await fetch(`/plug_ins/${encodeURIComponent(id)}/delete`, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
    },
    body,
  });
  if (!r.ok) alert(await r.text());
  else location.reload();
}

async function onSaveCode() {
  const id = window.__pluginState.currentId;
  if (!id) {
    alert("Select a plugin first");
    return;
  }
  const code = window.__pluginState.editor
    ? window.__pluginState.editor.getValue()
    : "";
  const body = toUrlEncoded({ code });
  const r = await fetch(`/plug_ins/${encodeURIComponent(id)}/save`, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
    },
    body,
  });
  if (!r.ok) alert(await r.text());
  else if (window.UIkit?.notification)
    UIkit.notification("Saved!", { status: "success" });
  else alert("Saved!");
}
function wireSidebar() {
  const list = document.getElementById("plugin_list");
  if (!list) return;

  list.addEventListener("click", (e) => {
    const delBtn = e.target.closest(".plugin-delete");
    if (delBtn) {
      const id = delBtn.getAttribute("data-plugin-id");
      if (id) onDelete(id);
      return;
    }

    const item = e.target.closest("[data-plugin-item]");
    if (!item) return;
    const id = item.getAttribute("data-plugin-id");
    const name = item.getAttribute("data-plugin-name") || "";
    const entry = item.getAttribute("data-plugin-entry") || "";
    const rt = item.getAttribute("data-plugin-runtime") || "python";
    setActivePlugin(id, name, entry, rt);
  });
}

function wireForms() {
  const fNew = document.getElementById("form_new");
  const fEdit = document.getElementById("form_edit");
  const saveBtn = document.getElementById("btn_save_code");

  if (fNew) fNew.addEventListener("submit", onCreate);
  if (fEdit) fEdit.addEventListener("submit", onUpdate);
  if (saveBtn) saveBtn.addEventListener("click", onSaveCode);
}

document.addEventListener("DOMContentLoaded", () => {
  initMonaco();
  wireSidebar();
  wireForms();
});
