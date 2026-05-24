/**
 * <pyana-app> — runtime context provider custom element.
 *
 * Usage:
 *   <pyana-app id="app"></pyana-app>
 *   <pyana-cell ref="pyana://cell/abc..."></pyana-cell>
 *
 * The cell element walks the DOM to find its nearest <pyana-app> ancestor and
 * reads `.runtime` from it. Set the runtime imperatively from page JS after
 * construction; inspectors that mount before the runtime is set wait for the
 * `pyana:runtime-ready` event.
 *
 * We don't use the (still-experimental) `<context-provider>` proposal yet —
 * the closest-ancestor walk is good enough and works in all browsers.
 */

class PyanaApp extends HTMLElement {
  constructor() {
    super();
    this._runtime = null;
  }
  get runtime() { return this._runtime; }
  set runtime(rt) {
    this._runtime = rt;
    // Notify any inspectors that mounted before us.
    this.dispatchEvent(new CustomEvent('pyana:runtime-ready', { detail: rt, bubbles: false }));
  }
}
if (!customElements.get('pyana-app')) customElements.define('pyana-app', PyanaApp);

/**
 * Walk up from `host` to find the enclosing <pyana-app>. If no runtime is
 * attached yet, wait for `pyana:runtime-ready`. Returns a Promise<Runtime>.
 */
export function findRuntime(host) {
  const app = host.closest('pyana-app');
  if (!app) return Promise.reject(new Error(`no <pyana-app> ancestor of <${host.localName}>`));
  if (app.runtime) return Promise.resolve(app.runtime);
  return new Promise(resolve => {
    app.addEventListener('pyana:runtime-ready', e => resolve(e.detail), { once: true });
  });
}
