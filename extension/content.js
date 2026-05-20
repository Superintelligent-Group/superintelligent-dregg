// Content script: bridges page.js (window.pyana) <-> background service worker.

const script = document.createElement('script');
script.src = chrome.runtime.getURL('page.js');
script.type = 'module';
(document.head || document.documentElement).appendChild(script);
script.onload = () => script.remove();

// Forward requests from page -> background.
window.addEventListener('pyana:request', async (event) => {
  const detail = event.detail;
  const response = await chrome.runtime.sendMessage(detail);
  window.dispatchEvent(new CustomEvent('pyana:response', { detail: response }));
});

// Forward event notifications from background -> page.
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'pyana:event') {
    window.dispatchEvent(new CustomEvent('pyana:event', {
      detail: { eventName: message.event, payload: message.payload },
    }));
    sendResponse({ ok: true });
  }
  return false;
});
