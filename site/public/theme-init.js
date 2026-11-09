/* Sets html[data-theme] before first paint: stored choice (same key Starlight uses), else the
   system preference, else dark. External file so the site needs no inline script (CSP script-src 'self'). */
(function () {
  var s = null;
  try { s = localStorage.getItem('starlight-theme'); } catch (e) {}
  var t = s || (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark');
  document.documentElement.dataset.theme = t === 'light' ? 'light' : 'dark';
})();
