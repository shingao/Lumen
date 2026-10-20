// Pose le thème avant le premier rendu pour éviter un flash (réglage en cache, sinon Windows).
(function () {
  var pref = "system";
  try {
    pref = localStorage.getItem("lumen-theme") || "system";
  } catch (e) {
    // Stockage indisponible.
  }
  var dark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
})();
