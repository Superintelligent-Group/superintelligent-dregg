//! Derivation step AIR constraint logic.
//!
//! This module contains ONLY the AIR constraint implementations.
//! All types, constants, witness structs, and prove/verify functions live
//! in [`crate::derivation_types`].
//!
//! For backward compatibility, everything from `derivation_types` is
//! re-exported here so existing `use crate::derivation_air::*` imports
//! continue to work.

use crate::constraint_prover::{Air, Constraint};
use crate::field::BabyBear;
use crate::poseidon2::hash_fact;
use crate::stark::{BoundaryConstraint, StarkAir};

// Re-export everything from derivation_types for backward compatibility.
pub use crate::derivation_types::*;

impl Air for DerivationAir {
    fn trace_width(&self) -> usize {
        DERIVATION_AIR_WIDTH
    }

    fn num_public_inputs(&self) -> usize {
        5 // state_root, derived_fact_hash, not_after_height, org_id_hash, budget_remaining
    }

    fn constraints(&self) -> Vec<Constraint> {
        vec![
            Constraint {
                name: "body_membership_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_BODY_ATOMS {
                        let flag = row[col::BODY_MEMBERSHIP_START + i];
                        result = result + flag * (flag - BabyBear::ONE);
                    }
                    result
                }),
            },
            Constraint {
                name: "body_hash_nonzero_when_used".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_BODY_ATOMS {
                        let flag = row[col::BODY_MEMBERSHIP_START + i];
                        let hash = row[col::BODY_HASH_START + i];
                        if flag == BabyBear::ONE && hash == BabyBear::ZERO {
                            result = result + BabyBear::ONE;
                        }
                    }
                    result
                }),
            },
            Constraint {
                name: "at_least_one_body".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut sum = BabyBear::ZERO;
                    for i in 0..MAX_BODY_ATOMS {
                        sum = sum + row[col::BODY_MEMBERSHIP_START + i];
                    }
                    if sum == BabyBear::ZERO {
                        BabyBear::ONE
                    } else {
                        BabyBear::ZERO
                    }
                }),
            },
            Constraint {
                name: "derived_hash_correct".to_string(),
                eval: Box::new(|row, _, _| {
                    let pred = row[col::HEAD_PRED];
                    let terms = [
                        row[col::HEAD_TERM_START],
                        row[col::HEAD_TERM_START + 1],
                        row[col::HEAD_TERM_START + 2],
                        row[col::HEAD_TERM_START + 3],
                    ];
                    let expected_hash = hash_fact(pred, &terms);
                    let claimed_hash = row[col::DERIVED_HASH];
                    expected_hash - claimed_hash
                }),
            },
            Constraint {
                name: "body_roots_match_state".to_string(),
                eval: Box::new(|row, _, public_inputs| {
                    let state_root = public_inputs[0];
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_BODY_ATOMS {
                        let flag = row[col::BODY_MEMBERSHIP_START + i];
                        let root = row[col::BODY_ROOT_START + i];
                        result = result + flag * (root - state_root);
                    }
                    result
                }),
            },
            Constraint {
                name: "derived_hash_public".to_string(),
                eval: Box::new(|row, _, public_inputs| row[col::DERIVED_HASH] - public_inputs[1]),
            },
            Constraint {
                name: "head_is_var_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_HEAD_TERMS {
                        let flag = row[col::HEAD_IS_VAR_START + i];
                        result = result + flag * (flag - BabyBear::ONE);
                    }
                    result
                }),
            },
            Constraint {
                name: "head_sel_var_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for term_i in 0..MAX_HEAD_TERMS {
                        for var_j in 0..MAX_SUB_VARS {
                            let sel = row[col::head_sel_var(term_i, var_j)];
                            result = result + sel * (sel - BabyBear::ONE);
                        }
                    }
                    result
                }),
            },
            Constraint {
                name: "head_sel_var_sum_equals_is_var".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for term_i in 0..MAX_HEAD_TERMS {
                        let is_var = row[col::HEAD_IS_VAR_START + term_i];
                        let mut sel_sum = BabyBear::ZERO;
                        for var_j in 0..MAX_SUB_VARS {
                            sel_sum = sel_sum + row[col::head_sel_var(term_i, var_j)];
                        }
                        result = result + (sel_sum - is_var) * (sel_sum - is_var);
                    }
                    result
                }),
            },
            Constraint {
                name: "substitution_application".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for term_i in 0..MAX_HEAD_TERMS {
                        let is_var = row[col::HEAD_IS_VAR_START + term_i];
                        let raw_value = row[col::HEAD_RAW_VALUE_START + term_i];
                        let derived_term = row[col::HEAD_TERM_START + term_i];

                        let mut var_resolved = BabyBear::ZERO;
                        for var_j in 0..MAX_SUB_VARS {
                            let sel = row[col::head_sel_var(term_i, var_j)];
                            let sub_val = row[col::SUB_VALUE_START + var_j];
                            var_resolved = var_resolved + sel * sub_val;
                        }

                        let expected = is_var * var_resolved + (BabyBear::ONE - is_var) * raw_value;
                        result = result + (derived_term - expected) * (derived_term - expected);
                    }
                    result
                }),
            },
            Constraint {
                name: "eq_check_active_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_EQUAL_CHECKS {
                        let active = row[col::eq_check_active(i)];
                        result = result + active * (active - BabyBear::ONE);
                    }
                    result
                }),
            },
            Constraint {
                name: "eq_check_enforced".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_EQUAL_CHECKS {
                        let active = row[col::eq_check_active(i)];
                        let term_a = row[col::eq_check_term_a(i)];
                        let term_b = row[col::eq_check_term_b(i)];
                        result = result + active * (term_a - term_b);
                    }
                    result
                }),
            },
            Constraint {
                name: "memberof_check_active_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_MEMBEROF_CHECKS {
                        let active = row[col::memberof_check_active(i)];
                        result = result + active * (active - BabyBear::ONE);
                    }
                    result
                }),
            },
            Constraint {
                name: "memberof_check_enforced".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for i in 0..MAX_MEMBEROF_CHECKS {
                        let active = row[col::memberof_check_active(i)];
                        let term_a = row[col::memberof_check_term_a(i)];
                        let term_b = row[col::memberof_check_term_b(i)];
                        result = result + active * (term_a - term_b);
                    }
                    result
                }),
            },
            Constraint {
                name: "gte_check_active_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::GTE_CHECK_ACTIVE];
                    active * (active - BabyBear::ONE)
                }),
            },
            Constraint {
                name: "gte_check_diff_correct".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::GTE_CHECK_ACTIVE];
                    let term_a = row[col::GTE_CHECK_TERM_A];
                    let term_b = row[col::GTE_CHECK_TERM_B];
                    let diff = row[col::GTE_CHECK_DIFF];
                    active * (diff - (term_a - term_b))
                }),
            },
            Constraint {
                name: "gte_check_bit_decomposition".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::GTE_CHECK_ACTIVE];
                    let diff = row[col::GTE_CHECK_DIFF];
                    let mut recomposed = BabyBear::ZERO;
                    let mut power_of_two = BabyBear::ONE;
                    for i in 0..GTE_DIFF_BITS {
                        let bit = row[col::gte_diff_bit(i)];
                        recomposed = recomposed + bit * power_of_two;
                        power_of_two = power_of_two + power_of_two;
                    }
                    active * (recomposed - diff)
                }),
            },
            Constraint {
                name: "gte_check_bits_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::GTE_CHECK_ACTIVE];
                    let mut result = BabyBear::ZERO;
                    for i in 0..GTE_DIFF_BITS {
                        let bit = row[col::gte_diff_bit(i)];
                        result = result + bit * (bit - BabyBear::ONE);
                    }
                    active * result
                }),
            },
            Constraint {
                name: "gte_check_high_bit_zero".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::GTE_CHECK_ACTIVE];
                    let high_bit = row[col::gte_diff_bit(GTE_DIFF_BITS - 1)];
                    active * high_bit
                }),
            },
            Constraint {
                name: "lt_check_active_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::LT_CHECK_ACTIVE];
                    active * (active - BabyBear::ONE)
                }),
            },
            Constraint {
                name: "lt_check_diff_correct".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::LT_CHECK_ACTIVE];
                    let term_a = row[col::LT_CHECK_TERM_A];
                    let term_b = row[col::LT_CHECK_TERM_B];
                    let diff = row[col::LT_CHECK_DIFF];
                    active * (diff - (term_b - term_a - BabyBear::ONE))
                }),
            },
            Constraint {
                name: "lt_check_bit_decomposition".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::LT_CHECK_ACTIVE];
                    let diff = row[col::LT_CHECK_DIFF];
                    let mut recomposed = BabyBear::ZERO;
                    let mut power_of_two = BabyBear::ONE;
                    for i in 0..GTE_DIFF_BITS {
                        let bit = row[col::lt_diff_bit(i)];
                        recomposed = recomposed + bit * power_of_two;
                        power_of_two = power_of_two + power_of_two;
                    }
                    active * (recomposed - diff)
                }),
            },
            Constraint {
                name: "lt_check_bits_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::LT_CHECK_ACTIVE];
                    let mut result = BabyBear::ZERO;
                    for i in 0..GTE_DIFF_BITS {
                        let bit = row[col::lt_diff_bit(i)];
                        result = result + bit * (bit - BabyBear::ONE);
                    }
                    active * result
                }),
            },
            Constraint {
                name: "lt_check_high_bit_zero".to_string(),
                eval: Box::new(|row, _, _| {
                    let active = row[col::LT_CHECK_ACTIVE];
                    let high_bit = row[col::lt_diff_bit(GTE_DIFF_BITS - 1)];
                    active * high_bit
                }),
            },
            Constraint {
                name: "check_term_is_var_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for slot in 0..col::NUM_CHECK_TERMS {
                        let is_var = row[col::check_term_is_var(slot)];
                        result = result + is_var * (is_var - BabyBear::ONE);
                    }
                    result
                }),
            },
            Constraint {
                name: "check_term_sel_binary".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for slot in 0..col::NUM_CHECK_TERMS {
                        for var_j in 0..MAX_SUB_VARS {
                            let sel = row[col::check_term_sel(slot, var_j)];
                            result = result + sel * (sel - BabyBear::ONE);
                        }
                    }
                    result
                }),
            },
            Constraint {
                name: "check_term_sel_sum_equals_is_var".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;
                    for slot in 0..col::NUM_CHECK_TERMS {
                        let is_var = row[col::check_term_is_var(slot)];
                        let mut sel_sum = BabyBear::ZERO;
                        for var_j in 0..MAX_SUB_VARS {
                            sel_sum = sel_sum + row[col::check_term_sel(slot, var_j)];
                        }
                        let diff = sel_sum - is_var;
                        result = result + diff * diff;
                    }
                    result
                }),
            },
            Constraint {
                name: "check_term_binding_correct".to_string(),
                eval: Box::new(|row, _, _| {
                    let mut result = BabyBear::ZERO;

                    let resolve_slot = |slot: usize| -> BabyBear {
                        let is_var = row[col::check_term_is_var(slot)];
                        let raw_value = row[col::check_term_raw_value(slot)];
                        let mut var_resolved = BabyBear::ZERO;
                        for var_j in 0..MAX_SUB_VARS {
                            let sel = row[col::check_term_sel(slot, var_j)];
                            let sub_val = row[col::SUB_VALUE_START + var_j];
                            var_resolved = var_resolved + sel * sub_val;
                        }
                        is_var * var_resolved + (BabyBear::ONE - is_var) * raw_value
                    };

                    for i in 0..MAX_EQUAL_CHECKS {
                        let active = row[col::eq_check_active(i)];
                        let trace_a = row[col::eq_check_term_a(i)];
                        let trace_b = row[col::eq_check_term_b(i)];
                        let resolved_a = resolve_slot(col::eq_check_term_a_slot(i));
                        let resolved_b = resolve_slot(col::eq_check_term_b_slot(i));
                        let diff_a = trace_a - resolved_a;
                        let diff_b = trace_b - resolved_b;
                        result = result + active * (diff_a * diff_a + diff_b * diff_b);
                    }

                    for i in 0..MAX_MEMBEROF_CHECKS {
                        let active = row[col::memberof_check_active(i)];
                        let trace_a = row[col::memberof_check_term_a(i)];
                        let trace_b = row[col::memberof_check_term_b(i)];
                        let resolved_a = resolve_slot(col::memberof_check_term_a_slot(i));
                        let resolved_b = resolve_slot(col::memberof_check_term_b_slot(i));
                        let diff_a = trace_a - resolved_a;
                        let diff_b = trace_b - resolved_b;
                        result = result + active * (diff_a * diff_a + diff_b * diff_b);
                    }

                    {
                        let active = row[col::GTE_CHECK_ACTIVE];
                        let trace_a = row[col::GTE_CHECK_TERM_A];
                        let trace_b = row[col::GTE_CHECK_TERM_B];
                        let resolved_a = resolve_slot(col::GTE_TERM_A_SLOT);
                        let resolved_b = resolve_slot(col::GTE_TERM_B_SLOT);
                        let diff_a = trace_a - resolved_a;
                        let diff_b = trace_b - resolved_b;
                        result = result + active * (diff_a * diff_a + diff_b * diff_b);
                    }

                    {
                        let active = row[col::LT_CHECK_ACTIVE];
                        let trace_a = row[col::LT_CHECK_TERM_A];
                        let trace_b = row[col::LT_CHECK_TERM_B];
                        let resolved_a = resolve_slot(col::LT_TERM_A_SLOT);
                        let resolved_b = resolve_slot(col::LT_TERM_B_SLOT);
                        let diff_a = trace_a - resolved_a;
                        let diff_b = trace_b - resolved_b;
                        result = result + active * (diff_a * diff_a + diff_b * diff_b);
                    }

                    result
                }),
            },
        ]
    }

    fn generate_trace(&self) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
        let w = &self.witness;
        let derived_hash = w.derived_hash();

        let mut row = vec![BabyBear::ZERO; DERIVATION_AIR_WIDTH];

        row[col::RULE_ID] = BabyBear::new(w.rule.id);

        for (i, &hash) in w.body_fact_hashes.iter().enumerate().take(MAX_BODY_ATOMS) {
            row[col::BODY_HASH_START + i] = hash;
            row[col::BODY_MEMBERSHIP_START + i] = BabyBear::ONE;
            row[col::BODY_ROOT_START + i] = w.state_root;
        }

        row[col::HEAD_PRED] = w.derived_predicate;
        for i in 0..MAX_HEAD_TERMS {
            row[col::HEAD_TERM_START + i] = w.derived_terms[i];
        }
        row[col::DERIVED_HASH] = derived_hash;

        for (i, &val) in w.substitution.iter().enumerate().take(MAX_SUB_VARS) {
            row[col::SUB_VALUE_START + i] = val;
        }

        for (term_i, &(is_var, value)) in w.rule.head_terms.iter().enumerate().take(MAX_HEAD_TERMS)
        {
            row[col::HEAD_IS_VAR_START + term_i] = if is_var {
                BabyBear::ONE
            } else {
                BabyBear::ZERO
            };
            row[col::HEAD_RAW_VALUE_START + term_i] = value;

            if is_var {
                let var_idx = value.as_u32() as usize;
                if var_idx < MAX_SUB_VARS {
                    row[col::head_sel_var(term_i, var_idx)] = BabyBear::ONE;
                }
            }
        }

        for (check_i, eq_check) in w
            .rule
            .equal_checks
            .iter()
            .enumerate()
            .take(MAX_EQUAL_CHECKS)
        {
            row[col::eq_check_active(check_i)] = BabyBear::ONE;

            let term_a = if eq_check.lhs_is_var {
                let idx = eq_check.lhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                eq_check.lhs_value
            };

            let term_b = if eq_check.rhs_is_var {
                let idx = eq_check.rhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                eq_check.rhs_value
            };

            row[col::eq_check_term_a(check_i)] = term_a;
            row[col::eq_check_term_b(check_i)] = term_b;
        }

        for (check_i, mo_check) in w
            .rule
            .memberof_checks
            .iter()
            .enumerate()
            .take(MAX_MEMBEROF_CHECKS)
        {
            row[col::memberof_check_active(check_i)] = BabyBear::ONE;

            let term_a = if mo_check.lhs_is_var {
                let idx = mo_check.lhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                mo_check.lhs_value
            };

            let term_b = if mo_check.rhs_is_var {
                let idx = mo_check.rhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                mo_check.rhs_value
            };

            row[col::memberof_check_term_a(check_i)] = term_a;
            row[col::memberof_check_term_b(check_i)] = term_b;
        }

        if let Some(gte_check) = &w.rule.gte_check {
            row[col::GTE_CHECK_ACTIVE] = BabyBear::ONE;

            let term_a = if gte_check.lhs_is_var {
                let idx = gte_check.lhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                gte_check.lhs_value
            };

            let term_b = if gte_check.rhs_is_var {
                let idx = gte_check.rhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                gte_check.rhs_value
            };

            row[col::GTE_CHECK_TERM_A] = term_a;
            row[col::GTE_CHECK_TERM_B] = term_b;

            let diff = term_a - term_b;
            row[col::GTE_CHECK_DIFF] = diff;

            let diff_val = diff.as_u32();
            for i in 0..GTE_DIFF_BITS {
                let bit = (diff_val >> i) & 1;
                row[col::gte_diff_bit(i)] = BabyBear::new(bit);
            }
        }

        if let Some(lt_check) = &w.rule.lt_check {
            row[col::LT_CHECK_ACTIVE] = BabyBear::ONE;

            let term_a = if lt_check.lhs_is_var {
                let idx = lt_check.lhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                lt_check.lhs_value
            };

            let term_b = if lt_check.rhs_is_var {
                let idx = lt_check.rhs_value.as_u32() as usize;
                if idx < w.substitution.len() {
                    w.substitution[idx]
                } else {
                    BabyBear::ZERO
                }
            } else {
                lt_check.rhs_value
            };

            row[col::LT_CHECK_TERM_A] = term_a;
            row[col::LT_CHECK_TERM_B] = term_b;

            let diff = term_b - term_a - BabyBear::ONE;
            row[col::LT_CHECK_DIFF] = diff;

            let diff_val = diff.as_u32();
            for i in 0..GTE_DIFF_BITS {
                let bit = (diff_val >> i) & 1;
                row[col::lt_diff_bit(i)] = BabyBear::new(bit);
            }
        }

        // Check term binding columns
        let fill_check_term_binding =
            |row: &mut Vec<BabyBear>, slot: usize, is_var: bool, value: BabyBear| {
                row[col::check_term_is_var(slot)] = if is_var {
                    BabyBear::ONE
                } else {
                    BabyBear::ZERO
                };
                row[col::check_term_raw_value(slot)] = value;
                if is_var {
                    let var_idx = value.as_u32() as usize;
                    if var_idx < MAX_SUB_VARS {
                        row[col::check_term_sel(slot, var_idx)] = BabyBear::ONE;
                    }
                }
            };

        for (check_i, eq_check) in w
            .rule
            .equal_checks
            .iter()
            .enumerate()
            .take(MAX_EQUAL_CHECKS)
        {
            fill_check_term_binding(
                &mut row,
                col::eq_check_term_a_slot(check_i),
                eq_check.lhs_is_var,
                eq_check.lhs_value,
            );
            fill_check_term_binding(
                &mut row,
                col::eq_check_term_b_slot(check_i),
                eq_check.rhs_is_var,
                eq_check.rhs_value,
            );
        }

        for (check_i, mo_check) in w
            .rule
            .memberof_checks
            .iter()
            .enumerate()
            .take(MAX_MEMBEROF_CHECKS)
        {
            fill_check_term_binding(
                &mut row,
                col::memberof_check_term_a_slot(check_i),
                mo_check.lhs_is_var,
                mo_check.lhs_value,
            );
            fill_check_term_binding(
                &mut row,
                col::memberof_check_term_b_slot(check_i),
                mo_check.rhs_is_var,
                mo_check.rhs_value,
            );
        }

        if let Some(gte_check) = &w.rule.gte_check {
            fill_check_term_binding(
                &mut row,
                col::GTE_TERM_A_SLOT,
                gte_check.lhs_is_var,
                gte_check.lhs_value,
            );
            fill_check_term_binding(
                &mut row,
                col::GTE_TERM_B_SLOT,
                gte_check.rhs_is_var,
                gte_check.rhs_value,
            );
        }

        if let Some(lt_check) = &w.rule.lt_check {
            fill_check_term_binding(
                &mut row,
                col::LT_TERM_A_SLOT,
                lt_check.lhs_is_var,
                lt_check.lhs_value,
            );
            fill_check_term_binding(
                &mut row,
                col::LT_TERM_B_SLOT,
                lt_check.rhs_is_var,
                lt_check.rhs_value,
            );
        }

        let public_inputs = vec![
            w.state_root,
            derived_hash,
            w.not_after_height,
            w.org_id_hash,
            w.budget_remaining,
        ];
        (vec![row], public_inputs)
    }
}

impl StarkAir for DerivationStarkAir {
    fn width(&self) -> usize {
        DERIVATION_AIR_WIDTH
    }

    fn constraint_degree(&self) -> usize {
        2
    }

    fn has_chain_continuity(&self) -> bool {
        false
    }

    fn air_name(&self) -> &'static str {
        "pyana-derivation-v1"
    }

    fn eval_constraints(
        &self,
        local: &[BabyBear],
        _next: &[BabyBear],
        public_inputs: &[BabyBear],
        alpha: BabyBear,
    ) -> BabyBear {
        let mut result = BabyBear::ZERO;
        let mut alpha_power = BabyBear::ONE;

        // C1: body_membership_binary
        for i in 0..MAX_BODY_ATOMS {
            let flag = local[col::BODY_MEMBERSHIP_START + i];
            result = result + alpha_power * (flag * (flag - BabyBear::ONE));
        }
        alpha_power = alpha_power * alpha;

        // C2: body_hash_nonzero_when_used
        for i in 0..MAX_BODY_ATOMS {
            let flag = local[col::BODY_MEMBERSHIP_START + i];
            let hash = local[col::BODY_HASH_START + i];
            if flag == BabyBear::ONE && hash == BabyBear::ZERO {
                result = result + alpha_power * BabyBear::ONE;
            }
        }
        alpha_power = alpha_power * alpha;

        // C3: at_least_one_body
        let mut sum = BabyBear::ZERO;
        for i in 0..MAX_BODY_ATOMS {
            sum = sum + local[col::BODY_MEMBERSHIP_START + i];
        }
        if sum == BabyBear::ZERO {
            result = result + alpha_power * BabyBear::ONE;
        }
        alpha_power = alpha_power * alpha;

        // C4: derived_hash_correct
        let pred = local[col::HEAD_PRED];
        let terms = [
            local[col::HEAD_TERM_START],
            local[col::HEAD_TERM_START + 1],
            local[col::HEAD_TERM_START + 2],
            local[col::HEAD_TERM_START + 3],
        ];
        let expected_hash = hash_fact(pred, &terms);
        result = result + alpha_power * (expected_hash - local[col::DERIVED_HASH]);
        alpha_power = alpha_power * alpha;

        // C5: body_roots_match_state
        let state_root = public_inputs[0];
        for i in 0..MAX_BODY_ATOMS {
            let flag = local[col::BODY_MEMBERSHIP_START + i];
            let root = local[col::BODY_ROOT_START + i];
            result = result + alpha_power * (flag * (root - state_root));
        }
        alpha_power = alpha_power * alpha;

        // C6: derived_hash_public
        result = result + alpha_power * (local[col::DERIVED_HASH] - public_inputs[1]);
        alpha_power = alpha_power * alpha;

        // C7: head_is_var_binary
        for i in 0..MAX_HEAD_TERMS {
            let flag = local[col::HEAD_IS_VAR_START + i];
            result = result + alpha_power * (flag * (flag - BabyBear::ONE));
        }
        alpha_power = alpha_power * alpha;

        // C8: head_sel_var_binary
        for term_i in 0..MAX_HEAD_TERMS {
            for var_j in 0..MAX_SUB_VARS {
                let sel = local[col::head_sel_var(term_i, var_j)];
                result = result + alpha_power * (sel * (sel - BabyBear::ONE));
            }
        }
        alpha_power = alpha_power * alpha;

        // C9: head_sel_var_sum_equals_is_var
        for term_i in 0..MAX_HEAD_TERMS {
            let is_var = local[col::HEAD_IS_VAR_START + term_i];
            let mut sel_sum = BabyBear::ZERO;
            for var_j in 0..MAX_SUB_VARS {
                sel_sum = sel_sum + local[col::head_sel_var(term_i, var_j)];
            }
            let diff = sel_sum - is_var;
            result = result + alpha_power * (diff * diff);
        }
        alpha_power = alpha_power * alpha;

        // C10: substitution_application
        for term_i in 0..MAX_HEAD_TERMS {
            let is_var = local[col::HEAD_IS_VAR_START + term_i];
            let raw_value = local[col::HEAD_RAW_VALUE_START + term_i];
            let derived_term = local[col::HEAD_TERM_START + term_i];

            let mut var_resolved = BabyBear::ZERO;
            for var_j in 0..MAX_SUB_VARS {
                let sel = local[col::head_sel_var(term_i, var_j)];
                let sub_val = local[col::SUB_VALUE_START + var_j];
                var_resolved = var_resolved + sel * sub_val;
            }

            let expected = is_var * var_resolved + (BabyBear::ONE - is_var) * raw_value;
            let diff = derived_term - expected;
            result = result + alpha_power * (diff * diff);
        }
        alpha_power = alpha_power * alpha;

        // C11: eq_check_active_binary
        for i in 0..MAX_EQUAL_CHECKS {
            let active = local[col::eq_check_active(i)];
            result = result + alpha_power * (active * (active - BabyBear::ONE));
        }
        alpha_power = alpha_power * alpha;

        // C12: eq_check_enforced
        for i in 0..MAX_EQUAL_CHECKS {
            let active = local[col::eq_check_active(i)];
            let term_a = local[col::eq_check_term_a(i)];
            let term_b = local[col::eq_check_term_b(i)];
            result = result + alpha_power * (active * (term_a - term_b));
        }
        alpha_power = alpha_power * alpha;

        // C13: memberof_check_active_binary
        for i in 0..MAX_MEMBEROF_CHECKS {
            let active = local[col::memberof_check_active(i)];
            result = result + alpha_power * (active * (active - BabyBear::ONE));
        }
        alpha_power = alpha_power * alpha;

        // C14: memberof_check_enforced
        for i in 0..MAX_MEMBEROF_CHECKS {
            let active = local[col::memberof_check_active(i)];
            let term_a = local[col::memberof_check_term_a(i)];
            let term_b = local[col::memberof_check_term_b(i)];
            result = result + alpha_power * (active * (term_a - term_b));
        }
        alpha_power = alpha_power * alpha;

        // C15: gte_check_active_binary
        let gte_active = local[col::GTE_CHECK_ACTIVE];
        result = result + alpha_power * (gte_active * (gte_active - BabyBear::ONE));
        alpha_power = alpha_power * alpha;

        // C16: gte_check_diff_correct
        let gte_term_a = local[col::GTE_CHECK_TERM_A];
        let gte_term_b = local[col::GTE_CHECK_TERM_B];
        let gte_diff = local[col::GTE_CHECK_DIFF];
        result = result + alpha_power * (gte_active * (gte_diff - (gte_term_a - gte_term_b)));
        alpha_power = alpha_power * alpha;

        // C17: gte_check_bit_decomposition
        {
            let mut recomposed = BabyBear::ZERO;
            let mut power_of_two = BabyBear::ONE;
            for i in 0..GTE_DIFF_BITS {
                let bit = local[col::gte_diff_bit(i)];
                recomposed = recomposed + bit * power_of_two;
                power_of_two = power_of_two + power_of_two;
            }
            result = result + alpha_power * (gte_active * (recomposed - gte_diff));
        }
        alpha_power = alpha_power * alpha;

        // C18: gte_check_bits_binary
        {
            let mut bits_result = BabyBear::ZERO;
            for i in 0..GTE_DIFF_BITS {
                let bit = local[col::gte_diff_bit(i)];
                bits_result = bits_result + bit * (bit - BabyBear::ONE);
            }
            result = result + alpha_power * (gte_active * bits_result);
        }
        alpha_power = alpha_power * alpha;

        // C19: gte_check_high_bit_zero
        let gte_high_bit = local[col::gte_diff_bit(GTE_DIFF_BITS - 1)];
        result = result + alpha_power * (gte_active * gte_high_bit);
        alpha_power = alpha_power * alpha;

        // C20: lt_check_active_binary
        let lt_active = local[col::LT_CHECK_ACTIVE];
        result = result + alpha_power * (lt_active * (lt_active - BabyBear::ONE));
        alpha_power = alpha_power * alpha;

        // C21: lt_check_diff_correct
        let lt_term_a = local[col::LT_CHECK_TERM_A];
        let lt_term_b = local[col::LT_CHECK_TERM_B];
        let lt_diff = local[col::LT_CHECK_DIFF];
        result = result
            + alpha_power * (lt_active * (lt_diff - (lt_term_b - lt_term_a - BabyBear::ONE)));
        alpha_power = alpha_power * alpha;

        // C22: lt_check_bit_decomposition
        {
            let mut recomposed = BabyBear::ZERO;
            let mut power_of_two = BabyBear::ONE;
            for i in 0..GTE_DIFF_BITS {
                let bit = local[col::lt_diff_bit(i)];
                recomposed = recomposed + bit * power_of_two;
                power_of_two = power_of_two + power_of_two;
            }
            result = result + alpha_power * (lt_active * (recomposed - lt_diff));
        }
        alpha_power = alpha_power * alpha;

        // C23: lt_check_bits_binary
        {
            let mut bits_result = BabyBear::ZERO;
            for i in 0..GTE_DIFF_BITS {
                let bit = local[col::lt_diff_bit(i)];
                bits_result = bits_result + bit * (bit - BabyBear::ONE);
            }
            result = result + alpha_power * (lt_active * bits_result);
        }
        alpha_power = alpha_power * alpha;

        // C24: lt_check_high_bit_zero
        let lt_high_bit = local[col::lt_diff_bit(GTE_DIFF_BITS - 1)];
        result = result + alpha_power * (lt_active * lt_high_bit);
        alpha_power = alpha_power * alpha;

        // C25: check_term_is_var_binary
        for slot in 0..col::NUM_CHECK_TERMS {
            let is_var = local[col::check_term_is_var(slot)];
            result = result + alpha_power * (is_var * (is_var - BabyBear::ONE));
        }
        alpha_power = alpha_power * alpha;

        // C26: check_term_sel_binary
        for slot in 0..col::NUM_CHECK_TERMS {
            for var_j in 0..MAX_SUB_VARS {
                let sel = local[col::check_term_sel(slot, var_j)];
                result = result + alpha_power * (sel * (sel - BabyBear::ONE));
            }
        }
        alpha_power = alpha_power * alpha;

        // C27: check_term_sel_sum_equals_is_var
        for slot in 0..col::NUM_CHECK_TERMS {
            let is_var = local[col::check_term_is_var(slot)];
            let mut sel_sum = BabyBear::ZERO;
            for var_j in 0..MAX_SUB_VARS {
                sel_sum = sel_sum + local[col::check_term_sel(slot, var_j)];
            }
            let diff = sel_sum - is_var;
            result = result + alpha_power * (diff * diff);
        }
        alpha_power = alpha_power * alpha;

        // C28: check_term_binding_correct
        let resolve_slot = |slot: usize| -> BabyBear {
            let is_var = local[col::check_term_is_var(slot)];
            let raw_value = local[col::check_term_raw_value(slot)];
            let mut var_resolved = BabyBear::ZERO;
            for var_j in 0..MAX_SUB_VARS {
                let sel = local[col::check_term_sel(slot, var_j)];
                let sub_val = local[col::SUB_VALUE_START + var_j];
                var_resolved = var_resolved + sel * sub_val;
            }
            is_var * var_resolved + (BabyBear::ONE - is_var) * raw_value
        };

        for i in 0..MAX_EQUAL_CHECKS {
            let active = local[col::eq_check_active(i)];
            let trace_a = local[col::eq_check_term_a(i)];
            let trace_b = local[col::eq_check_term_b(i)];
            let resolved_a = resolve_slot(col::eq_check_term_a_slot(i));
            let resolved_b = resolve_slot(col::eq_check_term_b_slot(i));
            let diff_a = trace_a - resolved_a;
            let diff_b = trace_b - resolved_b;
            result = result + alpha_power * (active * (diff_a * diff_a + diff_b * diff_b));
        }

        for i in 0..MAX_MEMBEROF_CHECKS {
            let active = local[col::memberof_check_active(i)];
            let trace_a = local[col::memberof_check_term_a(i)];
            let trace_b = local[col::memberof_check_term_b(i)];
            let resolved_a = resolve_slot(col::memberof_check_term_a_slot(i));
            let resolved_b = resolve_slot(col::memberof_check_term_b_slot(i));
            let diff_a = trace_a - resolved_a;
            let diff_b = trace_b - resolved_b;
            result = result + alpha_power * (active * (diff_a * diff_a + diff_b * diff_b));
        }

        {
            let active = local[col::GTE_CHECK_ACTIVE];
            let trace_a = local[col::GTE_CHECK_TERM_A];
            let trace_b = local[col::GTE_CHECK_TERM_B];
            let resolved_a = resolve_slot(col::GTE_TERM_A_SLOT);
            let resolved_b = resolve_slot(col::GTE_TERM_B_SLOT);
            let diff_a = trace_a - resolved_a;
            let diff_b = trace_b - resolved_b;
            result = result + alpha_power * (active * (diff_a * diff_a + diff_b * diff_b));
        }

        {
            let active = local[col::LT_CHECK_ACTIVE];
            let trace_a = local[col::LT_CHECK_TERM_A];
            let trace_b = local[col::LT_CHECK_TERM_B];
            let resolved_a = resolve_slot(col::LT_TERM_A_SLOT);
            let resolved_b = resolve_slot(col::LT_TERM_B_SLOT);
            let diff_a = trace_a - resolved_a;
            let diff_b = trace_b - resolved_b;
            result = result + alpha_power * (active * (diff_a * diff_a + diff_b * diff_b));
        }

        result
    }

    fn boundary_constraints(
        &self,
        public_inputs: &[BabyBear],
        _trace_len: usize,
    ) -> Vec<BoundaryConstraint> {
        let mut constraints = vec![];
        if public_inputs.len() >= 2 {
            constraints.push(BoundaryConstraint {
                row: 0,
                col: col::DERIVED_HASH,
                value: public_inputs[1],
            });
            constraints.push(BoundaryConstraint {
                row: 0,
                col: col::BODY_ROOT_START,
                value: public_inputs[0],
            });
        }
        constraints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint_prover::ConstraintProver;
    use crate::poseidon2::hash_fact;

    #[test]
    fn derivation_air_valid() {
        let witness = create_test_derivation();
        #[allow(deprecated)]
        let air = DerivationAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "Derivation AIR should verify: {:?}",
            result.violations()
        );
    }

    #[test]
    fn derivation_air_no_body_facts_fails() {
        let mut witness = create_test_derivation();
        witness.body_fact_hashes = vec![];
        #[allow(deprecated)]
        let air = DerivationAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(!result.is_valid());
    }

    #[test]
    fn derivation_witness_head_match() {
        let witness = create_test_derivation();
        assert!(witness.check_head_match());
    }

    #[test]
    fn test_derivation_air_gte_check_passes() {
        let access_pred = BabyBear::new(300);
        let owns_pred = BabyBear::new(100);
        let budget = BabyBear::new(50);
        let cost = BabyBear::new(10);

        let rule = CircuitRule {
            id: 6,
            num_body_atoms: 1,
            num_variables: 2,
            head_predicate: access_pred,
            head_terms: [
                (true, BabyBear::new(0)),
                (true, BabyBear::new(1)),
                (false, BabyBear::ZERO),
                (false, BabyBear::ZERO),
            ],
            body_atoms: vec![BodyAtomPattern {
                predicate: owns_pred,
                terms: [
                    (true, BabyBear::new(0)),
                    (true, BabyBear::new(1)),
                    (false, BabyBear::ZERO),
                ],
            }],
            equal_checks: vec![],
            memberof_checks: vec![],
            gte_check: Some(CircuitGteCheck {
                lhs_is_var: true,
                lhs_value: BabyBear::new(0),
                rhs_is_var: true,
                rhs_value: BabyBear::new(1),
            }),
            lt_check: None,
        };

        let body_fact = hash_fact(owns_pred, &[budget, cost, BabyBear::ZERO]);

        let witness = DerivationWitness {
            rule,
            state_root: BabyBear::new(99999),
            body_fact_hashes: vec![body_fact],
            substitution: vec![budget, cost],
            derived_predicate: access_pred,
            derived_terms: [budget, cost, BabyBear::ZERO, BabyBear::ZERO],
            not_after_height: BabyBear::ZERO,
            org_id_hash: BabyBear::ZERO,
            budget_remaining: BabyBear::ZERO,
        };

        #[allow(deprecated)]
        let air = DerivationAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "GTE check 50 >= 10 should pass: {:?}",
            result.violations()
        );
    }

    #[test]
    fn derivation_stark_proof_basic() {
        let witness = create_test_derivation();
        let proof =
            prove_derivation_stark(&witness).expect("derivation STARK proof should generate");

        #[allow(deprecated)]
        let air = DerivationAir::new(witness.clone());
        let (_, public_inputs) = air.generate_trace();
        assert!(
            verify_derivation_stark(&proof, &public_inputs).is_ok(),
            "derivation STARK proof should verify"
        );
    }
}
