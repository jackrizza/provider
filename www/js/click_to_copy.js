/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
(function () {
  function selectAll(el) {
    const range = document.createRange();
    range.selectNodeContents(el);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  }

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_) {
      // Fallback for older browsers / blocked clipboard
      try {
        const ta = document.createElement("textarea");
        ta.value = text;
        ta.setAttribute("readonly", "");
        ta.style.position = "absolute";
        ta.style.left = "-9999px";
        document.body.appendChild(ta);
        ta.select();
        const ok = document.execCommand("copy");
        document.body.removeChild(ta);
        return ok;
      } catch (_) {
        return false;
      }
    }
  }

  function notify(msg, ok) {
    if (window.UIkit && UIkit.notification) {
      UIkit.notification({
        message: msg,
        status: ok ? "success" : "danger",
        timeout: 1200,
      });
    } else {
      // Minimal fallback: console + quick flash
      console[ok ? "log" : "warn"]("[copy]", msg);
    }
  }

  function onDblClick(e) {
    const el = e.currentTarget;
    // Select the whole block (nice visual feedback)
    selectAll(el);

    // Copy trimmed text (keeps token exact; remove trim() if whitespace matters)
    const txt = el.textContent.trim();
    copyText(txt).then((ok) => {
      // notify(ok ? "Access token copied" : "Copy failed", ok);
      el.setAttribute("uk-tooltip", "Copied!");
    });
  }

  function init() {
    document.querySelectorAll(".copy-on-dblclick").forEach((el) => {
      el.addEventListener("dblclick", onDblClick);
      // Optional: make single-click select all too (comment out if you dislike it)
      // el.addEventListener("click", () => selectAll(el));
      // Improve UX hint via cursor
      el.style.cursor = "copy";
      // Make sure double-click selects everything even if wrapped
      el.style.userSelect = "text";
      // If your token has long strings, prevent layout shift while still wrapping if needed
      // (You already use uk-text-break; keep it.)
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();
