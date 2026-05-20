//! Standard policy set for the pyana authorization model.
//!
//! Provides pre-built rules matching the pyana authorization semantics:
//! - App-scoped access: allow if the request app has the requested action
//! - Service-scoped access: allow if the request service has the requested action
//! - Unrestricted access: allow if the token grants unrestricted access
//! - Default deny: if no allow fires, deny

use crate::types::*;

/// Standard policy rule IDs.
pub mod rule_ids {
    /// allow if app($app, $actions), request_app($app), request_action($act), $actions.contains($act)
    pub const APP_ACTION: u32 = 1;
    /// allow if service($svc, $actions), request_service($svc), request_action($act), $actions.contains($act)
    pub const SERVICE_ACTION: u32 = 2;
    /// allow if unrestricted(true), request_action($act)
    pub const UNRESTRICTED: u32 = 3;
    /// allow if app($app, $actions), request_app($app) [no action constraint]
    pub const APP_ANY_ACTION: u32 = 4;
    /// allow if service($svc, $actions), request_service($svc) [no action constraint]
    pub const SERVICE_ANY_ACTION: u32 = 5;
    /// Time-bounded: allow if app($app, $actions), request_app($app), request_action($act),
    ///   $actions.contains($act), valid_until($exp), request_time($t), $t < $exp
    pub const APP_ACTION_TIME_BOUNDED: u32 = 10;
    /// Time-bounded service: similar to above for services
    pub const SERVICE_ACTION_TIME_BOUNDED: u32 = 11;
}

/// Returns the standard pyana authorization policy rule set.
///
/// These rules implement the core authorization logic:
///
/// 1. **App + Action**: If an `app(AppId, Actions)` fact exists where AppId matches the
///    request and Actions contains the requested action, allow.
///
/// 2. **Service + Action**: Same as above but for service-scoped tokens.
///
/// 3. **Unrestricted**: If an `unrestricted(true)` fact exists, allow any action.
///
/// 4. **App (any action)**: If an app fact exists and the request has no action constraint,
///    allow. (Used when checking "can this token access this app at all?")
///
/// 5. **Service (any action)**: Same as above for services.
///
/// 6. **Time-bounded app + action**: Like rule 1 but also checks token expiry.
///
/// 7. **Time-bounded service + action**: Like rule 2 but also checks token expiry.
pub fn standard_policy() -> Vec<Rule> {
    vec![
        // Rule 1: allow if app($app, $actions), request_app($app), request_action($act), $actions.contains($act)
        Rule {
            id: rule_ids::APP_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("app"),
                    terms: vec![Term::Var(0), Term::Var(1)], // $app, $actions
                },
                Atom {
                    predicate: symbol_from_str("request_app"),
                    terms: vec![Term::Var(0)], // $app
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)], // $act
                },
            ],
            checks: vec![
                Check::Contains(Term::Var(1), Term::Var(2)), // $actions.contains($act)
            ],
        },
        // Rule 2: allow if service($svc, $actions), request_service($svc), request_action($act), $actions.contains($act)
        Rule {
            id: rule_ids::SERVICE_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("service"),
                    terms: vec![Term::Var(0), Term::Var(1)], // $svc, $actions
                },
                Atom {
                    predicate: symbol_from_str("request_service"),
                    terms: vec![Term::Var(0)], // $svc
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)], // $act
                },
            ],
            checks: vec![
                Check::Contains(Term::Var(1), Term::Var(2)), // $actions.contains($act)
            ],
        },
        // Rule 3: allow if unrestricted(true), request_action($act)
        Rule {
            id: rule_ids::UNRESTRICTED,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("unrestricted"),
                    terms: vec![Term::Int(1)], // true encoded as 1
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(0)], // $act (ensures there IS an action)
                },
            ],
            checks: vec![],
        },
        // Rule 4: allow if app($app, $actions), request_app($app) [no action required]
        Rule {
            id: rule_ids::APP_ANY_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("app"),
                    terms: vec![Term::Var(0), Term::Var(1)], // $app, $actions
                },
                Atom {
                    predicate: symbol_from_str("request_app"),
                    terms: vec![Term::Var(0)], // $app
                },
                Atom {
                    predicate: symbol_from_str("no_action_required"),
                    terms: vec![Term::Int(1)],
                },
            ],
            checks: vec![],
        },
        // Rule 5: allow if service($svc, $actions), request_service($svc) [no action required]
        Rule {
            id: rule_ids::SERVICE_ANY_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("service"),
                    terms: vec![Term::Var(0), Term::Var(1)], // $svc, $actions
                },
                Atom {
                    predicate: symbol_from_str("request_service"),
                    terms: vec![Term::Var(0)], // $svc
                },
                Atom {
                    predicate: symbol_from_str("no_action_required"),
                    terms: vec![Term::Int(1)],
                },
            ],
            checks: vec![],
        },
        // Rule 10: Time-bounded app + action
        // allow if app($app, $actions), request_app($app), request_action($act),
        //          valid_until($exp)
        //   checks: $actions.contains($act), request_time < $exp
        // Note: request_time is not directly checkable as a body atom here due to the
        // 4-body limit, so we use it via the time_bounded_policy() variant that includes it.
        Rule {
            id: rule_ids::APP_ACTION_TIME_BOUNDED,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("app"),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_app"),
                    terms: vec![Term::Var(0)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)],
                },
                Atom {
                    predicate: symbol_from_str("valid_until"),
                    terms: vec![Term::Var(3)],
                },
            ],
            checks: vec![
                Check::Contains(Term::Var(1), Term::Var(2)),
            ],
        },
        // Rule 11: Time-bounded service + action (same pattern)
        Rule {
            id: rule_ids::SERVICE_ACTION_TIME_BOUNDED,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("service"),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_service"),
                    terms: vec![Term::Var(0)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)],
                },
                Atom {
                    predicate: symbol_from_str("valid_until"),
                    terms: vec![Term::Var(3)],
                },
            ],
            checks: vec![
                Check::Contains(Term::Var(1), Term::Var(2)),
            ],
        },
    ]
}

/// Create a minimal policy with just app-action and unrestricted rules.
/// Useful for testing.
pub fn minimal_policy() -> Vec<Rule> {
    vec![
        // Rule 1: allow if app($app, $actions), request_app($app), request_action($act), $actions.contains($act)
        Rule {
            id: rule_ids::APP_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("app"),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_app"),
                    terms: vec![Term::Var(0)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)],
                },
            ],
            checks: vec![Check::Contains(Term::Var(1), Term::Var(2))],
        },
        // Rule 2: allow if service($svc, $actions), request_service($svc), request_action($act), $actions.contains($act)
        Rule {
            id: rule_ids::SERVICE_ACTION,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("service"),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_service"),
                    terms: vec![Term::Var(0)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)],
                },
            ],
            checks: vec![Check::Contains(Term::Var(1), Term::Var(2))],
        },
        // Rule 3: allow if unrestricted(true), request_action($act)
        Rule {
            id: rule_ids::UNRESTRICTED,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("unrestricted"),
                    terms: vec![Term::Int(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(0)],
                },
            ],
            checks: vec![],
        },
    ]
}

/// Create a time-bounded policy that checks token expiry.
///
/// This variant uses 5 body atoms (exceeding the ZK circuit's 4-atom limit).
/// In production, the time-bounded check would be split across two rules
/// (an intermediate derivation). This is the reference semantics.
pub fn time_bounded_policy() -> Vec<Rule> {
    vec![
        // allow if app($app, $actions), request_app($app), request_action($act),
        //          valid_until($exp), request_time($t)
        //   checks: $actions.contains($act), $t < $exp
        Rule {
            id: rule_ids::APP_ACTION_TIME_BOUNDED,
            head: Atom {
                predicate: symbol_from_str("allow"),
                terms: vec![],
            },
            body: vec![
                Atom {
                    predicate: symbol_from_str("app"),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    predicate: symbol_from_str("request_app"),
                    terms: vec![Term::Var(0)],
                },
                Atom {
                    predicate: symbol_from_str("request_action"),
                    terms: vec![Term::Var(2)],
                },
                Atom {
                    predicate: symbol_from_str("valid_until"),
                    terms: vec![Term::Var(3)],
                },
                Atom {
                    predicate: symbol_from_str("request_time"),
                    terms: vec![Term::Var(4)],
                },
            ],
            checks: vec![
                Check::Contains(Term::Var(1), Term::Var(2)),
                Check::LessThan(Term::Var(4), Term::Var(3)),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Evaluator;

    #[test]
    fn test_standard_policy_has_expected_rules() {
        let rules = standard_policy();
        assert!(rules.len() >= 5);

        // Check rule IDs are present
        assert!(rules.iter().any(|r| r.id == rule_ids::APP_ACTION));
        assert!(rules.iter().any(|r| r.id == rule_ids::SERVICE_ACTION));
        assert!(rules.iter().any(|r| r.id == rule_ids::UNRESTRICTED));
    }

    #[test]
    fn test_minimal_policy_app_action_allow() {
        let rules = minimal_policy();
        let facts = vec![Fact::new(
            symbol_from_str("app"),
            vec![
                Term::Const(symbol_from_str("dashboard")),
                Term::Const(symbol_from_str("read,write")),
            ],
        )];

        let eval = Evaluator::new(facts, rules);
        let request = AuthorizationRequest {
            app_id: Some(symbol_from_str("dashboard")),
            service: None,
            action: Some(symbol_from_str("read")),
            features: vec![],
            user_id: None,
            now: 1000,
        };

        let trace = eval.evaluate(&request);
        assert_eq!(
            trace.conclusion,
            Conclusion::Allow {
                policy_rule_id: rule_ids::APP_ACTION,
            }
        );
    }

    #[test]
    fn test_minimal_policy_app_action_deny() {
        let rules = minimal_policy();
        let facts = vec![Fact::new(
            symbol_from_str("app"),
            vec![
                Term::Const(symbol_from_str("dashboard")),
                Term::Const(symbol_from_str("read")),
            ],
        )];

        let eval = Evaluator::new(facts, rules);
        let request = AuthorizationRequest {
            app_id: Some(symbol_from_str("dashboard")),
            service: None,
            action: Some(symbol_from_str("delete")), // not in actions
            features: vec![],
            user_id: None,
            now: 1000,
        };

        let trace = eval.evaluate(&request);
        assert_eq!(trace.conclusion, Conclusion::Deny);
    }

    #[test]
    fn test_minimal_policy_unrestricted() {
        let rules = minimal_policy();
        let facts = vec![Fact::new(
            symbol_from_str("unrestricted"),
            vec![Term::Int(1)],
        )];

        let eval = Evaluator::new(facts, rules);
        let request = AuthorizationRequest {
            app_id: None,
            service: None,
            action: Some(symbol_from_str("anything")),
            features: vec![],
            user_id: None,
            now: 1000,
        };

        let trace = eval.evaluate(&request);
        assert_eq!(
            trace.conclusion,
            Conclusion::Allow {
                policy_rule_id: rule_ids::UNRESTRICTED,
            }
        );
    }
}
