/**
 * Apps view — application status cards (gallery, AMM, orderbook, etc).
 */

import { bus, state } from '../app.js';
import * as api from '../api.js';

export const name = 'apps';

export function init(el) {
  bus.on('apps:updated', (cells) => {
    if (state.currentPage === 'apps') renderAppStats(cells);
  });

  // Wire up app card clicks
  const grid = document.getElementById('apps-grid');
  if (grid) {
    grid.querySelectorAll('.app-card').forEach(card => {
      card.onclick = () => renderAppDetail(card.dataset.app);
    });
  }
}

export function update() {}
export function destroy() {}

function renderAppStats(cells) {
  const programCells = cells.filter(c => c.has_program);
  const totalBalance = cells.reduce((sum, c) => sum + (c.balance || 0), 0);

  document.getElementById('app-gallery-status').textContent = programCells.length > 0 ? programCells.length + ' contracts' : 'available';
  document.getElementById('app-amm-status').textContent = totalBalance > 0 ? api.formatNumber(totalBalance) + ' locked' : 'available';
  document.getElementById('app-orderbook-status').textContent = 'available';
  document.getElementById('app-lending-status').textContent = 'available';
  document.getElementById('app-stablecoin-status').textContent = 'available';
  document.getElementById('app-identity-status').textContent = 'available';
}

function renderAppDetail(appName) {
  const panel = document.getElementById('app-detail');
  const content = document.getElementById('app-detail-content');
  panel.hidden = false;

  const apps = {
    gallery: { title: 'Gallery Auctions', desc: 'NFT auctions with ZK ownership proofs. Each auction cell holds an asset commitment and accepts sealed bids.', fields: [['Auction Type', 'English, Dutch'], ['Ownership Proof', 'STARK membership over note tree'], ['Bid Mechanism', 'Sealed-bid commit-reveal'], ['Settlement', 'Atomic swap via conditional turns']] },
    amm: { title: 'AMM Pools', desc: 'Constant-product (x*y=k) liquidity pools. LPs deposit paired assets and earn fees from swaps.', fields: [['Pool Type', 'Constant-product (Uniswap v2)'], ['LP Tokens', 'Minted on deposit, burned on withdrawal'], ['Fee Model', '0.3% swap fee to LP holders'], ['Reserves', 'Stored as sovereign cell commitments']] },
    orderbook: { title: 'Orderbook', desc: 'On-chain limit order book with price-time priority matching.', fields: [['Order Types', 'Limit, market, fill-or-kill'], ['Matching', 'Price-time priority (continuous)'], ['Settlement', 'Atomic multi-party turns'], ['Cancellation', 'Bearer-auth revocation']] },
    lending: { title: 'Lending Positions', desc: 'Collateralized debt positions with liquidation thresholds.', fields: [['Position Type', 'Isolated margin CDPs'], ['Collateral Ratio', 'Configurable per asset pair'], ['Liquidation', 'Triggered by price oracle conditionals'], ['Interest', 'Block-based accrual via sovereign witness']] },
    stablecoin: { title: 'Stablecoin CDPs', desc: 'Algorithmic stablecoin backed by over-collateralized positions.', fields: [['Peg', '1:1 USD target'], ['Collateral', 'Multi-asset, configurable ratios'], ['Stability Fee', 'Per-epoch, collected on position close'], ['Health Factor', 'Computed from oracle + collateral ratio']] },
    identity: { title: 'Anonymous Credentials', desc: 'RBAC-based datalog credentials with ZK presentation.', fields: [['Schema', 'Datalog rules (issuer defines)'], ['Presentation', 'STARK proof of attribute satisfaction'], ['Revocation', 'Merkle non-membership proof'], ['Privacy', 'Zero-knowledge (no identity linkage)']] },
  };

  const app = apps[appName] || { title: appName, desc: 'Details unavailable.', fields: [] };
  content.innerHTML = `
    <h4>${app.title}</h4>
    <p style="font-size: 12px; color: var(--text-dim); margin-bottom: 16px; line-height: 1.6;">${app.desc}</p>
    <div class="detail-grid">
      ${app.fields.map(([label, value]) => `
        <span class="detail-grid__label">${label}</span>
        <span class="detail-grid__value">${value}</span>
      `).join('')}
    </div>
  `;
  document.getElementById('app-detail-close').onclick = () => panel.hidden = true;
}
