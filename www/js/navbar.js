/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
(function () {
  console.log("[navbar] script loaded"); // proves the file executed

  const QUERY = "(min-width: 640px)"; // UIkit @s

  function pickNavbar() {
    const desktop = document.getElementById("navbar");
    const mobile = document.getElementById("navbar-mobile");

    if (!desktop || !mobile) {
      console.warn("[navbar] elements missing", {
        desktop: !!desktop,
        mobile: !!mobile,
      });
      return;
    }

    const isDesktop = window.matchMedia(QUERY).matches;
    desktop.hidden = !isDesktop;
    mobile.hidden = isDesktop;

    console.log("[navbar] picked:", isDesktop ? "desktop" : "mobile");
  }

  // Run ASAP after parse (works with/without `defer`)
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", pickNavbar, { once: true });
  } else {
    // DOM already parsed
    queueMicrotask(pickNavbar);
  }

  // Keep it in sync on changes
  const mq = window.matchMedia(QUERY);
  mq.addEventListener
    ? mq.addEventListener("change", pickNavbar)
    : mq.addListener(pickNavbar); // old Safari

  let t;
  window.addEventListener(
    "resize",
    () => {
      clearTimeout(t);
      t = setTimeout(pickNavbar, 120);
    },
    { passive: true },
  );
  window.addEventListener("orientationchange", pickNavbar, { passive: true });

  // Optional: ensure offcanvas closes when switching up
  window.addEventListener(
    "resize",
    () => {
      const isDesktop = mq.matches;
      if (isDesktop && window.UIkit && UIkit.offcanvas) {
        try {
          UIkit.offcanvas("#mobile-offcanvas")?.hide();
        } catch (_) {}
      }
    },
    { passive: true },
  );
})();
