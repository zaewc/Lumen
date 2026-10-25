// Runs before the page is drawn, so it never flashes the other language.
// The page holds both languages; style.css shows the one in <html lang>.
// Order: the visitor's saved choice, then Korean if the browser lists it,
// otherwise English. Without JavaScript the page stays in Korean.
(() => {
  const root = document.documentElement;
  let lang = null;
  try {
    const saved = window.localStorage.getItem("lumen-site-lang");
    if (saved === "ko" || saved === "en") {
      lang = saved;
    }
  } catch {
    // Storage can be blocked (private windows, strict settings); fall through.
  }
  if (lang === null) {
    const preferred = Array.isArray(navigator.languages) ? navigator.languages : [];
    lang = preferred.length === 0 || preferred.some((tag) => /^ko(-|$)/i.test(tag)) ? "ko" : "en";
  }
  root.lang = lang;
  root.classList.add("js");
})();
