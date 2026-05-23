/**
 * Navigation component — handles page switching and keyboard shortcuts.
 */

import { bus, navigateTo, state } from '../app.js';

export function init() {
  const nav = document.getElementById('ex-nav');
  if (!nav) return;

  nav.addEventListener('click', (e) => {
    const item = e.target.closest('[data-page]');
    if (!item) return;
    navigateTo(item.dataset.page);
  });

  // Keyboard shortcuts: 1-9 for first 9 nav items, Escape to close panels
  document.addEventListener('keydown', (e) => {
    if (isInputFocused()) return;

    if (e.key === 'Escape') {
      document.querySelectorAll('.ex-detail-panel').forEach(el => el.hidden = true);
      document.getElementById('settings-modal').hidden = true;
      document.getElementById('search-input').blur();
    }

    // Number keys for quick nav
    if (/^[1-9]$/.test(e.key) && !e.ctrlKey && !e.metaKey && !e.altKey) {
      const items = document.querySelectorAll('.ex-nav__item');
      const idx = parseInt(e.key) - 1;
      if (items[idx]) {
        navigateTo(items[idx].dataset.page);
      }
    }
  });
}

function isInputFocused() {
  const tag = document.activeElement?.tagName?.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select';
}
