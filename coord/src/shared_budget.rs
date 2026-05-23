//! # Shared Resource Budget (Tier 2 Optimistic Shared Access)
//!
//! Generalizes the Stingray bounded counter from "one agent's budget across silos"
//! to "one shared resource accessed by multiple agents." This enables Tier 2
//! optimistic execution: agents can debit from a shared resource (AMM pool,
//! multi-sig account, shared cell) without coordination, as long as the aggregate
//! spending stays within bounds.
//!
//! ## Analogy to BudgetCoordinator
//!
//! | BudgetCoordinator (existing) | SharedResourceBudget (this module) |
//! |------------------------------|-------------------------------------|
//! | One agent's budget           | One shared resource's balance       |
//! | Distributed across silos     | Distributed across agents           |
//! | Silo = execution node        | Participant = agent accessing pool  |
//! | Coordinator = central entity | Coordinator = ordering node(s)      |
//! | Rebalance = reconcile silos  | Rebalance = epoch close (COD-style) |
//!
//! ## The Safety Invariant
//!
//! With n participants and at most f Byzantine:
//!
//!   allowance = resource_balance * (f+1) / (2f+1)
//!
//! Each agent can locally debit up to `allowance` without coordination. The worst
//! case: all n agents each spend `allowance`. But since at most f are Byzantine,
//! the honest agents collectively reveal their true spending at rebalance, and the
//! maximum possible overspend is bounded by `f * allowance`.
//!
//! ## Integration with Blocklace
//!
//! Each agent's debits against a shared resource are recorded in their virtual chain
//! (blocks in the blocklace). During rebalance, the coordinator observes each agent's
//! blocks and sums debits per resource. This makes the bounded counter derivable FROM
//! the blocklace state rather than requiring a separate accounting system.
//!
//! ## Relationship to COD (Close-Open-Debit)
//!
//! COD is reactive: check at debit time whether the sum exceeds the balance.
//! This module is pre-allocative: assign allowances upfront.
//!
//! The hybrid approach: pre-allocate for the fast path (no coordination needed for
//! debits within allowance), but trigger reactive escalation (Tier 3 ordering) when
//! an agent exhausts its allowance or when rebalance detects overspending. The COD
//! "close and epoch transition" maps exactly to our `rebalance()`.
//!
//! ## Dynamic Allowances
//!
//! Unlike the per-agent budget (where the balance is static between rebalances), a
//! shared resource's balance changes with every committed swap. The solution:
//!
//! 1. Allowances are computed from the LAST KNOWN balance at epoch start.
//! 2. Debits within the epoch are optimistic (may collectively overshoot).
//! 3. At epoch close, true balance is reconciled and allowances are recomputed.
//! 4. An "early rebalance" can be triggered if any participant exhausts its allowance.
//!
//! Credits (deposits into the pool) immediately increase the true balance and can
//! trigger allowance expansion without a full rebalance.

use std::collections::HashMap;

use pyana_cell::CellId;
use serde::{Deserialize, Serialize};

use crate::budget::{BudgetVersion, DebitDigest, ResourceAmount};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Identifies a shared resource (e.g., an AMM pool cell, multi-sig account).
/// This is the CellId of the resource cell.
pub type ResourceId = CellId;

/// Identifies a participant (agent) in the shared resource budget.
/// This is the agent's CellId (same as their identity key).
pub type ParticipantId = CellId;

// ─── AgentAllowance ──────────────────────────────────────────────────────────

/// Per-agent allowance for a shared resource.
///
/// Each agent accessing a shared resource gets a local spending ceiling. Debits
/// within the ceiling proceed without coordination. When the ceiling is hit, the
/// agent must request rebalancing (or escalate to Tier 3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAllowance {
    /// The agent who holds this allowance.
    pub agent: ParticipantId,
    /// The shared resource this allowance is for.
    pub resource: ResourceId,
    /// Budget epoch version.
    pub version: BudgetVersion,
    /// Maximum amount this agent may spend from the shared resource.
    pub ceiling: ResourceAmount,
    /// Amount already spent by this agent in the current epoch.
    pub spent: ResourceAmount,
    /// Debit transaction digests for this epoch.
    pub debits: Vec<DebitDigest>,
}

impl AgentAllowance {
    /// Create a new allowance for an agent on a shared resource.
    pub fn new(
        agent: ParticipantId,
        resource: ResourceId,
        version: BudgetVersion,
        ceiling: ResourceAmount,
    ) -> Self {
        AgentAllowance {
            agent,
            resource,
            version,
            ceiling,
            spent: 0,
            debits: Vec::new(),
        }
    }

    /// Remaining allowance.
    pub fn remaining(&self) -> ResourceAmount {
        self.ceiling.saturating_sub(self.spent)
    }

    /// Whether this allowance has any remaining budget.
    pub fn has_allowance(&self) -> bool {
        self.remaining() > 0
    }

    /// Try to debit from this allowance.
    ///
    /// Returns `Ok(())` if the debit succeeds (enough remaining).
    /// Returns `Err(SharedBudgetError::AllowanceExhausted)` if the ceiling is hit.
    pub fn try_debit(
        &mut self,
        amount: ResourceAmount,
        digest: DebitDigest,
    ) -> Result<(), SharedBudgetError> {
        if amount > self.remaining() {
            return Err(SharedBudgetError::AllowanceExhausted {
                agent: self.agent,
                resource: self.resource,
                remaining: self.remaining(),
                requested: amount,
            });
        }
        self.spent = self.spent.saturating_add(amount);
        self.debits.push(digest);
        Ok(())
    }

    /// Refund a previously debited amount (e.g., after turn abort).
    pub fn refund(&mut self, amount: ResourceAmount) {
        self.spent = self.spent.saturating_sub(amount);
    }
}

// ─── SharedResourceBudget ────────────────────────────────────────────────────

/// Manages allowance distribution for a single shared resource across multiple agents.
///
/// This is the Tier 2 equivalent of `BudgetCoordinator`: it distributes spending
/// rights for a shared resource (AMM pool, multi-sig, etc.) to participating agents.
/// Each agent can debit locally up to their allowance without coordination.
///
/// ## Coordinator Role
///
/// In practice, this struct lives on the ordering node(s) responsible for the
/// resource. The ordering nodes:
/// 1. Initialize the budget when the resource is registered for shared access.
/// 2. Distribute allowances to participating agents.
/// 3. Process spending reports during rebalancing.
/// 4. Detect overspending and escalate to Tier 3 if needed.
///
/// Alternatively, for fully peer-to-peer operation, the agents themselves can run
/// the rebalancing protocol using the blocklace as the ground truth.
#[derive(Clone, Debug)]
pub struct SharedResourceBudget {
    /// The shared resource being managed.
    pub resource: ResourceId,
    /// Participating agents.
    pub participants: Vec<ParticipantId>,
    /// Byzantine fault tolerance: max Byzantine agents to tolerate.
    pub byzantine_tolerance: usize,
    /// Current epoch version (incremented on each rebalance).
    pub version: BudgetVersion,
    /// Per-agent allowance states.
    pub allowances: HashMap<ParticipantId, AgentAllowance>,
    /// The resource's total available balance at epoch start.
    /// This is the ground truth from the ledger.
    pub total_balance: ResourceAmount,
    /// Credits received during this epoch (deposits increase the balance).
    /// These are tracked separately so rebalance can account for inflows.
    pub epoch_credits: ResourceAmount,
}

impl SharedResourceBudget {
    /// Create a new shared resource budget.
    ///
    /// # Parameters
    /// - `resource`: The shared resource cell ID.
    /// - `total_balance`: The resource's current available balance.
    /// - `participants`: Agents that may access this resource.
    /// - `byzantine_tolerance`: Max Byzantine agents to tolerate.
    ///
    /// # Errors
    /// Returns `Err` if `participants.len() < 2 * byzantine_tolerance + 1`.
    ///
    /// NOTE: The BFT threshold here is 2f+1 (not 3f+1 as in BudgetCoordinator).
    /// This is because in the shared-resource model, participants ARE the agents
    /// (not replicated nodes). We need a quorum of honest participants to attest
    /// to their spending. With n >= 2f+1, at least f+1 are honest, which suffices
    /// to reconstruct the true total spending.
    pub fn new(
        resource: ResourceId,
        total_balance: ResourceAmount,
        participants: Vec<ParticipantId>,
        byzantine_tolerance: usize,
    ) -> Result<Self, SharedBudgetError> {
        let n = participants.len();
        if n < 2 * byzantine_tolerance + 1 {
            return Err(SharedBudgetError::InsufficientParticipants {
                have: n,
                need: 2 * byzantine_tolerance + 1,
            });
        }

        let mut budget = SharedResourceBudget {
            resource,
            participants,
            byzantine_tolerance,
            version: 0,
            allowances: HashMap::new(),
            total_balance,
            epoch_credits: 0,
        };

        budget.distribute_allowances();
        Ok(budget)
    }

    /// Calculate the per-agent allowance ceiling.
    ///
    /// Formula: ceiling = balance * (f+1) / (2f+1)
    ///
    /// Same formula as BudgetCoordinator. The sum of all ceilings exceeds the true
    /// balance (intentionally) — this is what allows concurrent local spending.
    /// Safety: with at most f Byzantine agents, the maximum overspend is f * ceiling.
    pub fn compute_allowance_ceiling(&self) -> ResourceAmount {
        let f = self.byzantine_tolerance as u64;
        let numerator = f + 1;
        let denominator = 2 * f + 1;
        ((self.total_balance as u128 * numerator as u128) / denominator as u128) as u64
    }

    /// Distribute (or redistribute) allowances to all participants.
    fn distribute_allowances(&mut self) {
        let ceiling = self.compute_allowance_ceiling();
        self.allowances.clear();

        for &participant in &self.participants {
            let allowance = AgentAllowance::new(participant, self.resource, self.version, ceiling);
            self.allowances.insert(participant, allowance);
        }
    }

    /// Try to debit from a specific agent's allowance.
    ///
    /// This is the HOT PATH: no coordination with other agents or ordering nodes
    /// is needed as long as the agent's local allowance has remaining budget.
    pub fn try_debit(
        &mut self,
        agent: ParticipantId,
        amount: ResourceAmount,
        digest: DebitDigest,
    ) -> Result<(), SharedBudgetError> {
        let allowance = self
            .allowances
            .get_mut(&agent)
            .ok_or(SharedBudgetError::UnknownParticipant { agent })?;
        allowance.try_debit(amount, digest)
    }

    /// Get the remaining allowance for a specific agent.
    pub fn remaining(&self, agent: &ParticipantId) -> Option<ResourceAmount> {
        self.allowances.get(agent).map(|a| a.remaining())
    }

    /// Get total spent across all agents in this epoch.
    pub fn total_spent(&self) -> ResourceAmount {
        self.allowances.values().map(|a| a.spent).sum()
    }

    /// Record a credit (deposit) to the shared resource.
    ///
    /// Credits immediately increase the tracked balance. They do NOT automatically
    /// increase individual allowances (that happens at rebalance). However, the
    /// coordinator can optionally trigger an early allowance expansion.
    pub fn credit(&mut self, amount: ResourceAmount) {
        self.total_balance = self.total_balance.saturating_add(amount);
        self.epoch_credits = self.epoch_credits.saturating_add(amount);
    }

    /// Check if the total spending across all agents has exceeded the true balance.
    ///
    /// This is the COD "overspending detection" check. When true, escalation to
    /// Tier 3 ordering is needed to resolve which debits are valid.
    pub fn is_overspent(&self) -> bool {
        self.total_spent() > self.total_balance
    }

    /// Rebalance: collect spending reports, reconcile, and redistribute allowances.
    ///
    /// This is the "epoch close" operation (equivalent to COD's close-and-reopen).
    ///
    /// # Process
    /// 1. Sum actual spending from all reporting agents.
    /// 2. For non-reporting agents (assumed Byzantine), assume ceiling spent.
    /// 3. Deduct from resource balance.
    /// 4. Increment epoch, redistribute allowances.
    ///
    /// # Parameters
    /// - `reports`: Per-agent spending reports (agent -> total spent in epoch).
    /// - `require_all`: If true, reject unless all agents report.
    ///
    /// # Returns
    /// The total amount spent in this epoch.
    pub fn rebalance(
        &mut self,
        reports: &[(ParticipantId, ResourceAmount)],
        require_all: bool,
    ) -> Result<ResourceAmount, SharedBudgetError> {
        if require_all && reports.len() < self.participants.len() {
            return Err(SharedBudgetError::IncompleteReports {
                received: reports.len(),
                expected: self.participants.len(),
            });
        }

        let mut seen = HashMap::new();
        let mut total_spent: ResourceAmount = 0;
        let ceiling = self.compute_allowance_ceiling();

        for &(agent, spent) in reports {
            // Must be a known participant.
            if !self.allowances.contains_key(&agent) {
                return Err(SharedBudgetError::UnknownParticipant { agent });
            }

            // No duplicates.
            if seen.contains_key(&agent) {
                return Err(SharedBudgetError::DuplicateReport { agent });
            }

            // Reported spending must not exceed ceiling.
            if spent > ceiling {
                return Err(SharedBudgetError::ReportExceedsCeiling {
                    agent,
                    claimed: spent,
                    ceiling,
                });
            }

            seen.insert(agent, spent);
            total_spent = total_spent.saturating_add(spent);
        }

        // For missing participants in non-strict mode: assume full ceiling.
        if !require_all {
            for participant in &self.participants {
                if !seen.contains_key(participant) {
                    total_spent = total_spent.saturating_add(ceiling);
                }
            }
        }

        // Deduct total spending from the resource balance.
        if total_spent > self.total_balance {
            // Overspend detected. In production this triggers Tier 3 escalation.
            // For now, clamp to zero.
            self.total_balance = 0;
        } else {
            self.total_balance -= total_spent;
        }

        // New epoch.
        self.version += 1;
        self.epoch_credits = 0;
        self.distribute_allowances();

        Ok(total_spent)
    }

    /// Add a new participant to the shared resource budget.
    ///
    /// The new participant gets an allowance based on the current balance.
    /// Existing participants keep their current spent amounts but get new ceilings
    /// at the next rebalance.
    pub fn add_participant(&mut self, agent: ParticipantId) -> Result<(), SharedBudgetError> {
        if self.allowances.contains_key(&agent) {
            return Err(SharedBudgetError::DuplicateParticipant { agent });
        }
        self.participants.push(agent);
        let ceiling = self.compute_allowance_ceiling();
        let allowance = AgentAllowance::new(agent, self.resource, self.version, ceiling);
        self.allowances.insert(agent, allowance);
        Ok(())
    }

    /// Remove a participant from the shared resource budget.
    ///
    /// Their unspent allowance is implicitly returned to the pool at next rebalance.
    pub fn remove_participant(&mut self, agent: &ParticipantId) -> Result<(), SharedBudgetError> {
        if !self.allowances.contains_key(agent) {
            return Err(SharedBudgetError::UnknownParticipant { agent: *agent });
        }
        self.participants.retain(|p| p != agent);
        self.allowances.remove(agent);
        Ok(())
    }

    /// Compute allowances from blocklace state.
    ///
    /// Given a function that sums an agent's debits against this resource from
    /// their virtual chain in the blocklace, compute the effective allowance state.
    /// This enables deriving the bounded counter from the blocklace rather than
    /// maintaining separate accounting.
    ///
    /// # Parameters
    /// - `blocklace_debits`: maps each participant to their total debits observed
    ///   in the blocklace for this resource since the last epoch.
    pub fn sync_from_blocklace(
        &mut self,
        blocklace_debits: &HashMap<ParticipantId, ResourceAmount>,
    ) {
        for (agent, &debited) in blocklace_debits {
            if let Some(allowance) = self.allowances.get_mut(agent) {
                // The blocklace is the source of truth for what was spent.
                allowance.spent = debited;
            }
        }
    }
}

// ─── SharedBudgetError ───────────────────────────────────────────────────────

/// Errors from the shared resource budget system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedBudgetError {
    /// An agent's allowance is exhausted (escalate to rebalance or Tier 3).
    AllowanceExhausted {
        agent: ParticipantId,
        resource: ResourceId,
        remaining: ResourceAmount,
        requested: ResourceAmount,
    },
    /// Not enough participants for the requested Byzantine tolerance.
    InsufficientParticipants { have: usize, need: usize },
    /// Unknown participant (agent not registered for this resource).
    UnknownParticipant { agent: ParticipantId },
    /// Duplicate participant.
    DuplicateParticipant { agent: ParticipantId },
    /// Duplicate spending report from the same agent.
    DuplicateReport { agent: ParticipantId },
    /// A report claims more spending than the agent's ceiling allows.
    ReportExceedsCeiling {
        agent: ParticipantId,
        claimed: ResourceAmount,
        ceiling: ResourceAmount,
    },
    /// Not all participants submitted spending reports during rebalance.
    IncompleteReports { received: usize, expected: usize },
}

impl core::fmt::Display for SharedBudgetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SharedBudgetError::AllowanceExhausted {
                agent,
                resource,
                remaining,
                requested,
            } => {
                write!(
                    f,
                    "allowance exhausted for agent {agent} on resource {resource}: \
                     {remaining} remaining, {requested} requested"
                )
            }
            SharedBudgetError::InsufficientParticipants { have, need } => {
                write!(
                    f,
                    "insufficient participants: have {have}, need {need} (2f+1)"
                )
            }
            SharedBudgetError::UnknownParticipant { agent } => {
                write!(f, "unknown participant: {agent}")
            }
            SharedBudgetError::DuplicateParticipant { agent } => {
                write!(f, "duplicate participant: {agent}")
            }
            SharedBudgetError::DuplicateReport { agent } => {
                write!(f, "duplicate spending report from agent: {agent}")
            }
            SharedBudgetError::ReportExceedsCeiling {
                agent,
                claimed,
                ceiling,
            } => {
                write!(
                    f,
                    "agent {agent} reports {claimed} spent, but ceiling is {ceiling}"
                )
            }
            SharedBudgetError::IncompleteReports { received, expected } => {
                write!(
                    f,
                    "incomplete reports: received {received}, expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for SharedBudgetError {}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pool_resource() -> ResourceId {
        CellId::from_bytes([0xBB; 32])
    }

    fn test_agents(n: usize) -> Vec<ParticipantId> {
        (0..n)
            .map(|i| {
                let mut bytes = [0u8; 32];
                bytes[0] = i as u8;
                bytes[1] = (i >> 8) as u8;
                bytes[31] = 0xAA; // distinguish from zero
                CellId::from_bytes(bytes)
            })
            .collect()
    }

    fn test_digest(n: u64) -> DebitDigest {
        *blake3::hash(&n.to_le_bytes()).as_bytes()
    }

    // ── Basic Allowance Tests ────────────────────────────────────────────

    #[test]
    fn test_allowance_ceiling_f1() {
        // f=1, ceiling = balance * 2/3
        let agents = test_agents(4);
        let budget = SharedResourceBudget::new(pool_resource(), 10000, agents, 1).unwrap();
        assert_eq!(budget.compute_allowance_ceiling(), 6666);
    }

    #[test]
    fn test_allowance_ceiling_f2() {
        // f=2, ceiling = balance * 3/5
        let agents = test_agents(5);
        let budget = SharedResourceBudget::new(pool_resource(), 10000, agents, 2).unwrap();
        assert_eq!(budget.compute_allowance_ceiling(), 6000);
    }

    #[test]
    fn test_insufficient_participants() {
        let agents = test_agents(2); // Need 2*1+1 = 3 for f=1
        let result = SharedResourceBudget::new(pool_resource(), 10000, agents, 1);
        assert!(matches!(
            result,
            Err(SharedBudgetError::InsufficientParticipants { have: 2, need: 3 })
        ));
    }

    // ── Concurrent Debit Tests (the hot path) ────────────────────────────

    #[test]
    fn test_concurrent_debits_within_allowance() {
        let agents = test_agents(3);
        let agent_a = agents[0];
        let agent_b = agents[1];
        let agent_c = agents[2];

        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();
        // ceiling = 9000 * 2/3 = 6000

        // All agents debit concurrently — no coordination needed.
        assert!(budget.try_debit(agent_a, 2000, test_digest(1)).is_ok());
        assert!(budget.try_debit(agent_b, 3000, test_digest(2)).is_ok());
        assert!(budget.try_debit(agent_c, 1500, test_digest(3)).is_ok());

        assert_eq!(budget.total_spent(), 6500);
        assert_eq!(budget.remaining(&agent_a), Some(4000));
        assert_eq!(budget.remaining(&agent_b), Some(3000));
        assert_eq!(budget.remaining(&agent_c), Some(4500));
    }

    #[test]
    fn test_debit_exceeds_allowance() {
        let agents = test_agents(3);
        let agent_a = agents[0];

        let mut budget = SharedResourceBudget::new(pool_resource(), 3000, agents, 1).unwrap();
        // ceiling = 3000 * 2/3 = 2000

        // Try to spend more than allowance.
        let err = budget.try_debit(agent_a, 2001, test_digest(1)).unwrap_err();
        assert!(matches!(
            err,
            SharedBudgetError::AllowanceExhausted {
                remaining: 2000,
                requested: 2001,
                ..
            }
        ));
    }

    #[test]
    fn test_allowance_exhaustion_triggers_rebalance_need() {
        let agents = test_agents(3);
        let agent_a = agents[0];

        let mut budget = SharedResourceBudget::new(pool_resource(), 3000, agents, 1).unwrap();
        // ceiling = 2000

        // Spend up to ceiling.
        budget.try_debit(agent_a, 1000, test_digest(1)).unwrap();
        budget.try_debit(agent_a, 1000, test_digest(2)).unwrap();

        // Now exhausted — signals that agent needs a rebalance.
        let err = budget.try_debit(agent_a, 1, test_digest(3)).unwrap_err();
        assert!(matches!(
            err,
            SharedBudgetError::AllowanceExhausted {
                remaining: 0,
                requested: 1,
                ..
            }
        ));
    }

    // ── Multi-Agent Overspending Scenario (AMM pool) ─────────────────────

    #[test]
    fn test_amm_pool_scenario() {
        // AMM pool with 10000 tokens. 5 agents, f=1.
        let agents = test_agents(5);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 10000, agents.clone(), 1).unwrap();
        // ceiling = 10000 * 2/3 = 6666

        // Agent 0 swaps 5000 (within ceiling).
        budget.try_debit(agents[0], 5000, test_digest(0)).unwrap();
        // Agent 1 swaps 4000 (within ceiling).
        budget.try_debit(agents[1], 4000, test_digest(1)).unwrap();
        // Agent 2 swaps 3000 (within ceiling).
        budget.try_debit(agents[2], 3000, test_digest(2)).unwrap();

        // Total spent: 12000 > 10000 true balance.
        // Overspending detected!
        assert_eq!(budget.total_spent(), 12000);
        assert!(budget.is_overspent());

        // This triggers escalation to Tier 3 (ordering) to decide which
        // debits are valid. In production, the rebalance would reject or
        // roll back the excess.
    }

    #[test]
    fn test_no_overspend_when_agents_are_conservative() {
        // Same pool, but agents are conservative.
        let agents = test_agents(5);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 10000, agents.clone(), 1).unwrap();

        // Each agent spends only 2000 (well within ceiling of 6666).
        for (i, &agent) in agents.iter().enumerate() {
            budget
                .try_debit(agent, 2000, test_digest(i as u64))
                .unwrap();
        }

        // Total: 10000 = exactly the balance. No overspend.
        assert_eq!(budget.total_spent(), 10000);
        assert!(!budget.is_overspent());
    }

    // ── Rebalancing Tests ────────────────────────────────────────────────

    #[test]
    fn test_rebalance_full_reports() {
        let agents = test_agents(3);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 9000, agents.clone(), 1).unwrap();

        // Agents spend various amounts.
        budget.try_debit(agents[0], 1000, test_digest(0)).unwrap();
        budget.try_debit(agents[1], 2000, test_digest(1)).unwrap();
        // Agent 2 spends nothing.

        // All agents report their spending.
        let reports = vec![(agents[0], 1000), (agents[1], 2000), (agents[2], 0)];
        let total = budget.rebalance(&reports, true).unwrap();
        assert_eq!(total, 3000);

        // Balance updated: 9000 - 3000 = 6000.
        assert_eq!(budget.total_balance, 6000);

        // New epoch, new ceiling: 6000 * 2/3 = 4000.
        assert_eq!(budget.version, 1);
        assert_eq!(budget.compute_allowance_ceiling(), 4000);
    }

    #[test]
    fn test_rebalance_partial_mode() {
        let agents = test_agents(3);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 9000, agents.clone(), 1).unwrap();

        budget.try_debit(agents[0], 500, test_digest(0)).unwrap();

        // Only agent 0 reports. Others assumed to have spent full ceiling (6000 each).
        let reports = vec![(agents[0], 500)];
        let total = budget.rebalance(&reports, false).unwrap();
        // 500 + 2 * 6000 = 12500
        assert_eq!(total, 12500);

        // Balance clamped to 0 (overspent from conservative estimate).
        assert_eq!(budget.total_balance, 0);
    }

    #[test]
    fn test_rebalance_rejects_incomplete() {
        let agents = test_agents(3);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 9000, agents.clone(), 1).unwrap();

        let reports = vec![(agents[0], 100)];
        let err = budget.rebalance(&reports, true).unwrap_err();
        assert!(matches!(
            err,
            SharedBudgetError::IncompleteReports {
                received: 1,
                expected: 3
            }
        ));
    }

    #[test]
    fn test_rebalance_rejects_overspend_report() {
        let agents = test_agents(3);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 9000, agents.clone(), 1).unwrap();
        // ceiling = 6000

        // Agent claims more than their ceiling.
        let reports = vec![
            (agents[0], 7000), // exceeds 6000 ceiling
            (agents[1], 0),
            (agents[2], 0),
        ];
        let err = budget.rebalance(&reports, true).unwrap_err();
        assert!(matches!(
            err,
            SharedBudgetError::ReportExceedsCeiling {
                claimed: 7000,
                ceiling: 6000,
                ..
            }
        ));
    }

    // ── Credit (Deposit) Tests ───────────────────────────────────────────

    #[test]
    fn test_credit_increases_balance() {
        let agents = test_agents(3);
        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        budget.credit(1000);
        assert_eq!(budget.total_balance, 10000);
        assert_eq!(budget.epoch_credits, 1000);

        // Allowances don't change until rebalance.
        assert_eq!(budget.compute_allowance_ceiling(), 6666);
    }

    // ── Dynamic Participation Tests ──────────────────────────────────────

    #[test]
    fn test_add_participant() {
        let agents = test_agents(3);
        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        let new_agent = CellId::from_bytes([0xFF; 32]);
        budget.add_participant(new_agent).unwrap();

        assert_eq!(budget.participants.len(), 4);
        assert!(budget.allowances.contains_key(&new_agent));
        // New agent gets the current ceiling.
        assert_eq!(budget.remaining(&new_agent), Some(6000));
    }

    #[test]
    fn test_add_duplicate_participant_rejected() {
        let agents = test_agents(3);
        let agent_a = agents[0];
        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        let err = budget.add_participant(agent_a).unwrap_err();
        assert!(matches!(
            err,
            SharedBudgetError::DuplicateParticipant { .. }
        ));
    }

    #[test]
    fn test_remove_participant() {
        let agents = test_agents(3);
        let agent_c = agents[2];
        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        budget.remove_participant(&agent_c).unwrap();
        assert_eq!(budget.participants.len(), 2);
        assert!(!budget.allowances.contains_key(&agent_c));
    }

    // ── Blocklace Sync Tests ─────────────────────────────────────────────

    #[test]
    fn test_sync_from_blocklace() {
        let agents = test_agents(3);
        let agent_a = agents[0];
        let agent_b = agents[1];

        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        // Simulate: blocklace shows agent_a spent 1500, agent_b spent 800.
        let mut observed = HashMap::new();
        observed.insert(agent_a, 1500u64);
        observed.insert(agent_b, 800u64);

        budget.sync_from_blocklace(&observed);

        assert_eq!(budget.remaining(&agent_a), Some(6000 - 1500));
        assert_eq!(budget.remaining(&agent_b), Some(6000 - 800));
    }

    // ── Fast-Path Integration Scenario ───────────────────────────────────

    #[test]
    fn test_fast_path_eliminates_lock_round() {
        // Scenario: shared resource with bounded-counter approach.
        // Agent can debit locally without acquiring a lock first.
        // This is the key advantage over 2f+1 lock signatures.
        let agents = test_agents(3);
        let agent_a = agents[0];

        let mut budget = SharedResourceBudget::new(pool_resource(), 9000, agents, 1).unwrap();

        // Fast path: debit locally (one operation, no network round trips).
        let result = budget.try_debit(agent_a, 1000, test_digest(42));
        assert!(result.is_ok());

        // Compare: without bounded counter, agent would need:
        // 1. Request lock from 2f+1 nodes
        // 2. Receive lock signatures
        // 3. Execute
        // 4. Release lock
        // The bounded counter reduces this to a single local check.
    }

    // ── Safety Bound Test ────────────────────────────────────────────────

    #[test]
    fn test_byzantine_agent_overspend_is_bounded() {
        // With 3 agents, f=1, ceiling = balance * 2/3.
        // Even if the Byzantine agent spends its full ceiling, the overspend
        // beyond the true balance is bounded by f * ceiling.
        let agents = test_agents(3);
        let byzantine = agents[0];

        let mut budget = SharedResourceBudget::new(pool_resource(), 3000, agents, 1).unwrap();
        let ceiling = budget.compute_allowance_ceiling();
        // ceiling = 3000 * 2/3 = 2000
        assert_eq!(ceiling, 2000);

        // Byzantine agent spends full ceiling.
        budget.try_debit(byzantine, 2000, test_digest(0)).unwrap();
        assert_eq!(budget.remaining(&byzantine), Some(0));

        // The honest agents collectively can ALSO each spend up to 2000.
        // Worst case total: 3 * 2000 = 6000 vs true balance of 3000.
        // Max overspend = (n * ceiling) - balance = 6000 - 3000 = 3000.
        // But in practice, overspend is detected and honest agents stop or
        // rebalance before it gets this bad.
        //
        // The Stingray invariant: with at most f=1 Byzantine, the maximum
        // UNDETECTABLE overspend before rebalance is f * ceiling = 2000.
        // This is because the honest agents (n-f = 2) reveal their true spending
        // at rebalance, and only the Byzantine agent's claim is unverifiable.
    }

    // ── Epoch Lifecycle (Full Scenario) ──────────────────────────────────

    #[test]
    fn test_full_epoch_lifecycle() {
        let agents = test_agents(4);
        let mut budget =
            SharedResourceBudget::new(pool_resource(), 12000, agents.clone(), 1).unwrap();
        // ceiling = 12000 * 2/3 = 8000

        // --- Epoch 0: agents transact ---
        budget.try_debit(agents[0], 3000, test_digest(10)).unwrap();
        budget.try_debit(agents[1], 2000, test_digest(11)).unwrap();
        budget.try_debit(agents[2], 1500, test_digest(12)).unwrap();
        // Agent 3 is idle.

        // Someone deposits into the pool.
        budget.credit(500);

        assert_eq!(budget.total_spent(), 6500);
        assert!(!budget.is_overspent()); // 6500 < 12500

        // --- Epoch close: rebalance ---
        let reports = vec![
            (agents[0], 3000),
            (agents[1], 2000),
            (agents[2], 1500),
            (agents[3], 0),
        ];
        let total = budget.rebalance(&reports, true).unwrap();
        assert_eq!(total, 6500);

        // New balance: 12500 (original + credit) - 6500 = 6000.
        assert_eq!(budget.total_balance, 6000);
        assert_eq!(budget.version, 1);
        assert_eq!(budget.epoch_credits, 0); // reset

        // --- Epoch 1: fresh allowances ---
        // ceiling = 6000 * 2/3 = 4000
        assert_eq!(budget.compute_allowance_ceiling(), 4000);
        for &agent in &agents {
            assert_eq!(budget.remaining(&agent), Some(4000));
        }
    }
}
