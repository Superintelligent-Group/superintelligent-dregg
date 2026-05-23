//! Boot checks: verify the PyanaEngine starts and is operational.

use pyana_sdk::{EngineConfig, PyanaEngine};

use crate::report::{CheckResult, run_check};

pub fn run() -> Vec<CheckResult> {
    vec![
        run_check("node_healthy", check_node_healthy),
        run_check("height_advances", check_height_advances),
    ]
}

fn check_node_healthy() -> Result<(), String> {
    let engine = PyanaEngine::new(EngineConfig::for_testing());
    // Engine created successfully and ledger is empty (healthy state)
    if !engine.ledger().is_empty() {
        return Err("fresh engine should have empty ledger".into());
    }
    if engine.federation_root() != [0u8; 32] {
        return Err("fresh engine should have zero federation root".into());
    }
    Ok(())
}

fn check_height_advances() -> Result<(), String> {
    let mut engine = PyanaEngine::new(EngineConfig::for_testing());
    engine.set_block_height(0);

    // Simulate block advancement
    engine.set_block_height(1);
    engine.set_block_height(2);
    engine.set_block_height(3);

    // Verify via executor
    if engine.executor().block_height != 3 {
        return Err(format!(
            "expected block height 3, got {}",
            engine.executor().block_height
        ));
    }
    Ok(())
}
