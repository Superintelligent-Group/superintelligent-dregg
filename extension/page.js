// Injected into page context. Defines window.pyana API.

const pending = new Map();
let idCounter = 0;

function sendMessage(type, payload) {
  return new Promise((resolve, reject) => {
    const id = `pyana_${Date.now()}_${idCounter++}`;
    pending.set(id, { resolve, reject });
    window.dispatchEvent(new CustomEvent('pyana:request', {
      detail: { type, id, ...payload },
    }));
    setTimeout(() => {
      if (pending.has(id)) {
        pending.delete(id);
        reject(new Error('Pyana: request timed out'));
      }
    }, 30000);
  });
}

window.addEventListener('pyana:response', (event) => {
  const detail = event.detail;
  const resolver = pending.get(detail.id);
  if (!resolver) return;
  pending.delete(detail.id);
  if (detail.error) {
    resolver.reject(new Error(detail.error));
  } else {
    resolver.resolve(detail.result);
  }
});

// ---------------------------------------------------------------------------
// Event system
// ---------------------------------------------------------------------------

const eventListeners = new Map(); // event -> Set<callback>

function addListener(event, callback) {
  if (typeof callback !== 'function') {
    throw new TypeError('pyana.on: callback must be a function');
  }
  const validEvents = ['ready', 'authorization', 'revoked'];
  if (!validEvents.includes(event)) {
    throw new Error(`pyana.on: unknown event "${event}". Valid: ${validEvents.join(', ')}`);
  }
  if (!eventListeners.has(event)) {
    eventListeners.set(event, new Set());
    // Subscribe to this event type in the background
    sendMessage('pyana:subscribe', { event }).catch(() => {});
  }
  eventListeners.get(event).add(callback);
}

function removeListener(event, callback) {
  const listeners = eventListeners.get(event);
  if (listeners) {
    listeners.delete(callback);
  }
}

// Listen for event notifications forwarded from content script.
window.addEventListener('pyana:event', (event) => {
  const { eventName, payload } = event.detail;
  const listeners = eventListeners.get(eventName);
  if (listeners) {
    for (const cb of listeners) {
      try { cb(payload); } catch (e) { console.error('[pyana] event handler error:', e); }
    }
  }
});

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

const pyana = {
  authorize(request) {
    return sendMessage('pyana:authorize', { request });
  },

  isConnected() {
    return sendMessage('pyana:isConnected').then(() => true).catch(() => false);
  },

  getCapabilities() {
    return sendMessage('pyana:getCapabilities');
  },

  /**
   * Provision a capability token into the wallet.
   * The extension will show a confirmation dialog to the user.
   *
   * @param {Uint8Array|object} tokenBytes - Token data. If an object, it should
   *   have: { actions: string[], resource: string, expiry?: number, issuer?: string }
   * @returns {Promise<{accepted: boolean, tokenId?: string}>}
   */
  provision(tokenBytes) {
    let tokenData;
    if (tokenBytes instanceof Uint8Array) {
      // Decode token bytes — for now treat as JSON-encoded token descriptor.
      try {
        tokenData = JSON.parse(new TextDecoder().decode(tokenBytes));
      } catch (e) {
        return Promise.reject(new Error('pyana.provision: invalid token bytes'));
      }
    } else if (tokenBytes && typeof tokenBytes === 'object') {
      tokenData = tokenBytes;
    } else {
      return Promise.reject(new Error('pyana.provision: tokenBytes must be Uint8Array or object'));
    }
    return sendMessage('pyana:provision', { tokenData });
  },

  /**
   * Register an event listener.
   *
   * @param {'ready'|'authorization'|'revoked'} event
   * @param {function} callback
   */
  on(event, callback) {
    addListener(event, callback);
  },

  /**
   * Remove an event listener.
   *
   * @param {'ready'|'authorization'|'revoked'} event
   * @param {function} callback
   */
  off(event, callback) {
    removeListener(event, callback);
  },
};

Object.defineProperty(window, 'pyana', {
  value: Object.freeze(pyana),
  writable: false,
  configurable: false,
});

window.dispatchEvent(new Event('pyana:ready'));
