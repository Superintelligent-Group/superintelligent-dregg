//! Turn execution checks: transfer, set_field, grant, multi-effect, nonce, conservation.

use pyana_cell::{AuthRequired, CapabilityRef, Cell, Ledger, Permissions};
use pyana_turn::{ComputronCosts, DelegationMode, Effect, TurnBuilder, TurnExecutor, TurnResult};

use crate::report::{CheckResult, run_check};

fn test_key(name: &str) -> [u8; 32] {
    *blake3::hash(format!("preflight-turns:{name}").as_bytes()).as_bytes()
}

fn open_permissions() -> Permissions {
    Permissions {
        send: AuthRequired::None,
        receive: AuthRequired::None,
        set_state: AuthRequired::None,
        set_permissions: AuthRequired::None,
        set_verification_key: AuthRequired::None,
        increment_nonce: AuthRequired::None,
        delegate: AuthRequired::None,
        access: AuthRequired::None,
    }
}

pub fn run() -> Vec<CheckResult> {
    vec![
        run_check("transfer", check_transfer),
        run_check("setfield", check_set_field),
        run_check("grant", check_grant_capability),
        run_check("multi_effect", check_multi_effect),
        run_check("nonce", check_nonce_increments),
        run_check("conservation", check_conservation_law),
    ]
}

fn check_transfer() -> Result<(), String> {
    let token_id = test_key("token");
    let mut ledger = Ledger::new();

    let alice_key = test_key("alice");
    let mut alice = Cell::with_balance(alice_key, token_id, 1000);
    alice.permissions = open_permissions();
    let alice_id = alice.id;
    ledger.insert_cell(alice).map_err(|e| format!("{e:?}"))?;

    let bob_key = test_key("bob");
    let mut bob = Cell::with_balance(bob_key, token_id, 0);
    bob.permissions = open_permissions();
    let bob_id = bob.id;
    ledger.insert_cell(bob).map_err(|e| format!("{e:?}"))?;

    // Grant alice capability to bob
    {
        let a = ledger.get_mut(&alice_id).unwrap();
        a.capabilities.grant(bob_id, AuthRequired::None);
    }

    let executor = TurnExecutor::new(ComputronCosts::default_costs());
    let mut tb = TurnBuilder::new(alice_id, 0);
    tb.set_fee(1000);
    {
        let action = tb.action(bob_id, "transfer");
        action.delegation(DelegationMode::None);
        action.effect(Effect::Transfer {
            from: alice_id,
            to: bob_id,
            amount: 200,
        });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    match result {
        TurnResult::Committed { .. } => {}
        TurnResult::Rejected { reason, .. } => {
            return Err(format!("transfer rejected: {reason}"));
        }
        _ => return Err("unexpected turn result".into()),
    }

    let bob_cell = ledger.get(&bob_id).ok_or("bob not found")?;
    if bob_cell.state.balance != 200 {
        return Err(format!(
            "expected bob balance 200, got {}",
            bob_cell.state.balance
        ));
    }

    Ok(())
}

fn check_set_field() -> Result<(), String> {
    let token_id = test_key("token-sf");
    let mut ledger = Ledger::new();

    let owner_key = test_key("owner-sf");
    let mut cell = Cell::with_balance(owner_key, token_id, 10000);
    cell.permissions = open_permissions();
    let cell_id = cell.id;
    ledger.insert_cell(cell).map_err(|e| format!("{e:?}"))?;

    let executor = TurnExecutor::new(ComputronCosts::default_costs());
    let mut tb = TurnBuilder::new(cell_id, 0);
    tb.set_fee(100);
    {
        let action = tb.action(cell_id, "setfield");
        action.delegation(DelegationMode::None);
        action.effect(Effect::SetField {
            cell: cell_id,
            index: 3,
            value: *blake3::hash(b"my-data").as_bytes(),
        });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    match result {
        TurnResult::Committed { .. } => {}
        TurnResult::Rejected { reason, .. } => {
            return Err(format!("setfield rejected: {reason}"));
        }
        _ => return Err("unexpected turn result".into()),
    }

    let updated = ledger.get(&cell_id).ok_or("cell not found")?;
    if updated.state.fields[3] != *blake3::hash(b"my-data").as_bytes() {
        return Err("field 3 not updated correctly".into());
    }

    Ok(())
}

fn check_grant_capability() -> Result<(), String> {
    let token_id = test_key("token-gc");
    let mut ledger = Ledger::new();

    let granter_key = test_key("granter");
    let mut granter = Cell::with_balance(granter_key, token_id, 10000);
    granter.permissions = open_permissions();
    let granter_id = granter.id;
    ledger.insert_cell(granter).map_err(|e| format!("{e:?}"))?;

    let target_key = test_key("target-gc");
    let mut target = Cell::with_balance(target_key, token_id, 0);
    target.permissions = open_permissions();
    let target_id = target.id;
    ledger.insert_cell(target).map_err(|e| format!("{e:?}"))?;

    let executor = TurnExecutor::new(ComputronCosts::default_costs());
    let mut tb = TurnBuilder::new(granter_id, 0);
    tb.set_fee(100);
    {
        let action = tb.action(granter_id, "grant");
        action.delegation(DelegationMode::None);
        action.effect(Effect::GrantCapability {
            from: granter_id,
            to: granter_id,
            cap: CapabilityRef {
                target: target_id,
                slot: 0,
                permissions: AuthRequired::None,
                breadstuff: None,
                expires_at: None,
                allowed_effects: None,
            },
        });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    match result {
        TurnResult::Committed { .. } => {}
        TurnResult::Rejected { reason, .. } => {
            return Err(format!("grant rejected: {reason}"));
        }
        _ => return Err("unexpected turn result".into()),
    }

    // Verify capability is in c-list
    let g = ledger.get(&granter_id).ok_or("granter not found")?;
    if !g.capabilities.has_access(&target_id) {
        return Err("granter should have capability to target after grant".into());
    }

    Ok(())
}

fn check_multi_effect() -> Result<(), String> {
    let token_id = test_key("token-me");
    let mut ledger = Ledger::new();

    let owner_key = test_key("owner-me");
    let mut owner = Cell::with_balance(owner_key, token_id, 50000);
    owner.permissions = open_permissions();
    let owner_id = owner.id;
    ledger.insert_cell(owner).map_err(|e| format!("{e:?}"))?;

    let target_key = test_key("target-me");
    let mut target = Cell::with_balance(target_key, token_id, 0);
    target.permissions = open_permissions();
    let target_id = target.id;
    ledger.insert_cell(target).map_err(|e| format!("{e:?}"))?;

    // Grant owner cap to target
    {
        let o = ledger.get_mut(&owner_id).unwrap();
        o.capabilities.grant(target_id, AuthRequired::None);
    }

    let executor = TurnExecutor::new(ComputronCosts::default_costs());
    let mut tb = TurnBuilder::new(owner_id, 0);
    tb.set_fee(1000);
    {
        let action = tb.action(target_id, "multi");
        action.delegation(DelegationMode::None);
        // Multiple effects in one action
        action.effect(Effect::Transfer {
            from: owner_id,
            to: target_id,
            amount: 100,
        });
        action.effect(Effect::SetField {
            cell: target_id,
            index: 0,
            value: *blake3::hash(b"multi-effect-1").as_bytes(),
        });
        action.effect(Effect::SetField {
            cell: target_id,
            index: 1,
            value: *blake3::hash(b"multi-effect-2").as_bytes(),
        });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    match result {
        TurnResult::Committed { .. } => {}
        TurnResult::Rejected { reason, .. } => {
            return Err(format!("multi-effect rejected: {reason}"));
        }
        _ => return Err("unexpected turn result".into()),
    }

    let t = ledger.get(&target_id).ok_or("target not found")?;
    if t.state.balance != 100 {
        return Err(format!(
            "expected target balance 100, got {}",
            t.state.balance
        ));
    }
    if t.state.fields[0] != *blake3::hash(b"multi-effect-1").as_bytes() {
        return Err("field 0 not set".into());
    }
    if t.state.fields[1] != *blake3::hash(b"multi-effect-2").as_bytes() {
        return Err("field 1 not set".into());
    }

    Ok(())
}

fn check_nonce_increments() -> Result<(), String> {
    let token_id = test_key("token-nonce");
    let mut ledger = Ledger::new();

    let owner_key = test_key("owner-nonce");
    let mut owner = Cell::with_balance(owner_key, token_id, 50000);
    owner.permissions = open_permissions();
    let owner_id = owner.id;
    ledger.insert_cell(owner).map_err(|e| format!("{e:?}"))?;

    let executor = TurnExecutor::new(ComputronCosts::default_costs());

    // Execute turn with nonce=0
    let mut tb = TurnBuilder::new(owner_id, 0);
    tb.set_fee(100);
    {
        let action = tb.action(owner_id, "noop");
        action.delegation(DelegationMode::None);
        action.effect(Effect::IncrementNonce { cell: owner_id });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    if !matches!(result, TurnResult::Committed { .. }) {
        return Err("first turn should commit".into());
    }

    let after = ledger.get(&owner_id).ok_or("cell not found")?;
    if after.state.nonce != 1 {
        return Err(format!(
            "expected nonce=1 after increment, got {}",
            after.state.nonce
        ));
    }

    Ok(())
}

fn check_conservation_law() -> Result<(), String> {
    let token_id = test_key("token-cons");
    let mut ledger = Ledger::new();

    let alice_key = test_key("alice-cons");
    let mut alice = Cell::with_balance(alice_key, token_id, 500);
    alice.permissions = open_permissions();
    let alice_id = alice.id;
    ledger.insert_cell(alice).map_err(|e| format!("{e:?}"))?;

    let bob_key = test_key("bob-cons");
    let mut bob = Cell::with_balance(bob_key, token_id, 0);
    bob.permissions = open_permissions();
    let bob_id = bob.id;
    ledger.insert_cell(bob).map_err(|e| format!("{e:?}"))?;

    {
        let a = ledger.get_mut(&alice_id).unwrap();
        a.capabilities.grant(bob_id, AuthRequired::None);
    }

    let executor = TurnExecutor::new(ComputronCosts::default_costs());

    // Attempt to transfer more than balance (should be rejected)
    let mut tb = TurnBuilder::new(alice_id, 0);
    tb.set_fee(100);
    {
        let action = tb.action(bob_id, "transfer");
        action.delegation(DelegationMode::None);
        action.effect(Effect::Transfer {
            from: alice_id,
            to: bob_id,
            amount: 100_000, // more than alice has
        });
    }
    let turn = tb.build();
    let result = executor.execute(&turn, &mut ledger);
    match result {
        TurnResult::Rejected { .. } => {
            // Good: conservation law enforced
        }
        TurnResult::Committed { .. } => {
            return Err(
                "transfer exceeding balance should be rejected (conservation law violated)".into(),
            );
        }
        _ => return Err("unexpected turn result".into()),
    }

    // Verify balances unchanged
    let a = ledger.get(&alice_id).ok_or("alice not found")?;
    let b = ledger.get(&bob_id).ok_or("bob not found")?;
    if a.state.balance != 500 || b.state.balance != 0 {
        return Err("balances should be unchanged after rejected turn".into());
    }

    Ok(())
}
