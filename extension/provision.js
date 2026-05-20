// Provision popup script — decodes token data and handles accept/reject.

const params = new URLSearchParams(window.location.search);
let tokenData = null;

try {
  tokenData = JSON.parse(decodeURIComponent(params.get('data') || '{}'));
} catch (e) {
  tokenData = {};
}

// Populate token details.
const issuerEl = document.getElementById('issuer');
const resourceEl = document.getElementById('resource');
const actionsEl = document.getElementById('actions');
const expiryEl = document.getElementById('expiry');
const warningEl = document.getElementById('warning');

issuerEl.textContent = tokenData.issuer || 'Unknown';
resourceEl.textContent = tokenData.resource || '*';

if (Array.isArray(tokenData.actions) && tokenData.actions.length > 0) {
  actionsEl.innerHTML = tokenData.actions
    .map(a => `<span class="action-tag">${escapeHtml(a)}</span>`)
    .join('');
} else {
  actionsEl.innerHTML = '<span class="action-tag">all</span>';
  warningEl.textContent = 'Warning: This token grants ALL actions. Only accept if you trust the issuer.';
  warningEl.style.display = 'block';
}

if (tokenData.expiry) {
  const expiryDate = new Date(tokenData.expiry);
  if (expiryDate < new Date()) {
    expiryEl.innerHTML = `<span class="expired">Expired: ${expiryDate.toLocaleString()}</span>`;
    warningEl.textContent = 'Warning: This token is already expired.';
    warningEl.style.display = 'block';
  } else {
    expiryEl.textContent = expiryDate.toLocaleString();
  }
} else {
  expiryEl.textContent = 'Never';
}

// Resource wildcard warning.
if (tokenData.resource === '*' || !tokenData.resource) {
  if (!warningEl.textContent) {
    warningEl.textContent = 'Warning: This token applies to ALL resources.';
    warningEl.style.display = 'block';
  }
}

// Buttons.
document.getElementById('acceptBtn').addEventListener('click', () => {
  chrome.runtime.sendMessage({
    type: 'pyana:provisionDecision',
    accepted: true,
    tokenData,
  });
  window.close();
});

document.getElementById('rejectBtn').addEventListener('click', () => {
  chrome.runtime.sendMessage({
    type: 'pyana:provisionDecision',
    accepted: false,
    tokenData,
  });
  window.close();
});

// If the popup is closed without clicking, treat as rejection.
window.addEventListener('beforeunload', () => {
  chrome.runtime.sendMessage({
    type: 'pyana:provisionDecision',
    accepted: false,
    tokenData,
  });
});

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
