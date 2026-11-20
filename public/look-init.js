// Runs before first paint (same rules as src/logic/look.ts applyLook).
(function () {
  var r = document.documentElement, t = "default", a = "system";
  try {
    t = localStorage.getItem("theme") === "blocky" ? "blocky" : "default";
    a = localStorage.getItem("appearance") || "system";
  } catch (e) {}
  r.dataset.theme = t;
  if (t === "default" && (a === "dark" || (a !== "light" && matchMedia("(prefers-color-scheme: dark)").matches)))
    r.classList.add("dark");
})();
