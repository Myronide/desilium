/* Mockup helper: theme from ?theme=light|dark (or saved choice); ?shot hides dev controls. */
(function () {
  const query = new URLSearchParams(location.search);
  const root = document.documentElement;
  let saved = null;
  try {
    saved = localStorage.getItem('ds-theme');
  } catch (error) {
    saved = null;
  }
  root.setAttribute('data-theme', query.get('theme') || saved || 'dark');
  if (query.has('shot')) {
    root.classList.add('shot');
  }
  window.dsToggleTheme = function () {
    const next = root.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    try {
      localStorage.setItem('ds-theme', next);
    } catch (error) {
      // Storage can be blocked (private mode); the toggle still works for this page view.
    }
  };
})();
