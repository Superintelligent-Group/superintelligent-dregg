//! Production DSL predicate circuits.
//!
//! This module contains the canonical predicate proving/verification implementations.
//! These replace the old hand-written AIRs in `circuit/src/predicate_air.rs`,
//! `circuit/src/relational_predicate_air.rs`, `circuit/src/arithmetic_predicate_air.rs`,
//! and `circuit/src/compound_predicate_air.rs`.
//!
//! The DSL versions are MORE CAPABLE than the originals (blinding, commitment derivation,
//! all operators, gate trees, threshold K-of-N).

pub mod arithmetic;
pub mod base;
pub mod compound;
pub mod relational;

// Re-export primary types from each sub-module.
pub use base::{
    PredicateOp, PredicateWitness, compute_blinded_fact_commitment, compute_fact_commitment,
    generate_in_range_traces, generate_predicate_trace, generate_predicate_trace_full,
    predicate_descriptor, prove_predicate_dsl, verify_predicate_dsl,
};

pub use relational::{
    RelationalOp, RelationalWitness, compute_commitment as compute_value_commitment,
    generate_relational_trace, generate_relational_trace_full, prove_relational_dsl,
    relational_predicate_descriptor, verify_relational_dsl,
};

pub use arithmetic::{
    ArithExpr, ArithPredicate, CompareOp, CompiledArith, build_arithmetic_predicate_descriptor,
    compile_expression, evaluate_compiled_slots, generate_full_trace as generate_arithmetic_trace,
    prove_arithmetic_dsl, verify_arithmetic_dsl,
};

pub use compound::{
    BooleanFormula, CompoundOp, Gate, compound_predicate_circuit_descriptor,
    compound_predicate_dsl_circuit, compute_sub_proof_commitment, compute_tree_hash,
    evaluate_formula, generate_compound_trace, generate_compound_trace_full,
    generate_custom_gate_trace, generate_nested_trace, generate_threshold_trace,
    prove_compound_dsl, verify_compound_dsl,
};
