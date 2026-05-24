// starbridge-apps/shared/inspectors/index.js
//
// Inspector registry for starbridge-apps. Each app contributes its
// domain inspectors (Preact components published as ES modules — see
// site/STUDIO.md §6) and registers them via `window.pyana.register`.
//
// Today: empty stub. The first concrete inspectors land alongside
// `starbridge-apps/nameservice/` (the file `name.js` in this
// directory once written), then `auction.js`, `proposal.js`,
// `credential.js`, etc., as each starbridge-app comes online per
// STARBRIDGE-APPS-PLAN.md §6.
//
// Once an inspector file exists, register it here:
//
//   import { NameInspector, NameRegistryInspector } from './name.js';
//   window.pyana?.register?.('pyana-name', NameInspector);
//   window.pyana?.register?.('pyana-name-registry', NameRegistryInspector);
//
// The wasm runtime + Studio context resolve `<pyana-name uri="...">`
// custom elements through this registry.

export const registry = {
  // app-name -> { tag-name -> component }
};

export function register(app, tag, component) {
  if (!registry[app]) registry[app] = {};
  registry[app][tag] = component;
  if (typeof window !== 'undefined' && window.pyana?.register) {
    window.pyana.register(tag, component);
  }
}
