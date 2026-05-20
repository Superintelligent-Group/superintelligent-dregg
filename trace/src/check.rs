//! Constraint check evaluation, shared between the evaluator and verifier.

use crate::types::*;

/// Evaluate a single constraint check against a substitution.
pub fn eval_check(check: &Check, subst: &Substitution) -> bool {
    match check {
        Check::LessThan(lhs, rhs) => {
            let l = subst.apply_term(lhs);
            let r = subst.apply_term(rhs);
            matches!((&l, &r), (Term::Int(a), Term::Int(b)) if a < b)
        }
        Check::GreaterThan(lhs, rhs) => {
            let l = subst.apply_term(lhs);
            let r = subst.apply_term(rhs);
            matches!((&l, &r), (Term::Int(a), Term::Int(b)) if a > b)
        }
        Check::Equal(lhs, rhs) => {
            let l = subst.apply_term(lhs);
            let r = subst.apply_term(rhs);
            l == r
        }
        Check::Contains(collection, element) => {
            let col = subst.apply_term(collection);
            let elem = subst.apply_term(element);
            eval_contains(&col, &elem)
        }
    }
}

/// Evaluate the "contains" check.
///
/// Semantics: the collection symbol (interpreted as a UTF-8 string, zero-trimmed)
/// contains the element symbol as a substring. Equality also satisfies containment.
fn eval_contains(collection: &Term, element: &Term) -> bool {
    match (collection, element) {
        (Term::Const(c), Term::Const(e)) => {
            if c == e {
                return true;
            }
            let c_str = core::str::from_utf8(c)
                .unwrap_or("")
                .trim_end_matches('\0');
            let e_str = core::str::from_utf8(e)
                .unwrap_or("")
                .trim_end_matches('\0');
            if !e_str.is_empty() {
                c_str.contains(e_str)
            } else {
                false
            }
        }
        (Term::Int(c), Term::Int(e)) => c == e,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_from_str;

    #[test]
    fn test_less_than_pass() {
        let check = Check::LessThan(Term::Var(0), Term::Int(1000));
        let subst = Substitution::empty().extend(0, Term::Int(500)).unwrap();
        assert!(eval_check(&check, &subst));
    }

    #[test]
    fn test_less_than_fail() {
        let check = Check::LessThan(Term::Var(0), Term::Int(1000));
        let subst = Substitution::empty().extend(0, Term::Int(2000)).unwrap();
        assert!(!eval_check(&check, &subst));
    }

    #[test]
    fn test_greater_than() {
        let check = Check::GreaterThan(Term::Int(100), Term::Var(0));
        let subst = Substitution::empty().extend(0, Term::Int(50)).unwrap();
        assert!(eval_check(&check, &subst));
    }

    #[test]
    fn test_equal_pass() {
        let check = Check::Equal(Term::Var(0), Term::Const(symbol_from_str("hello")));
        let subst = Substitution::empty()
            .extend(0, Term::Const(symbol_from_str("hello")))
            .unwrap();
        assert!(eval_check(&check, &subst));
    }

    #[test]
    fn test_equal_fail() {
        let check = Check::Equal(Term::Var(0), Term::Const(symbol_from_str("hello")));
        let subst = Substitution::empty()
            .extend(0, Term::Const(symbol_from_str("world")))
            .unwrap();
        assert!(!eval_check(&check, &subst));
    }

    #[test]
    fn test_contains_exact_match() {
        let check = Check::Contains(
            Term::Const(symbol_from_str("read")),
            Term::Const(symbol_from_str("read")),
        );
        assert!(eval_check(&check, &Substitution::empty()));
    }

    #[test]
    fn test_contains_substring() {
        let check = Check::Contains(
            Term::Const(symbol_from_str("read,write,delete")),
            Term::Const(symbol_from_str("write")),
        );
        assert!(eval_check(&check, &Substitution::empty()));
    }

    #[test]
    fn test_contains_miss() {
        let check = Check::Contains(
            Term::Const(symbol_from_str("read,write")),
            Term::Const(symbol_from_str("delete")),
        );
        assert!(!eval_check(&check, &Substitution::empty()));
    }

    #[test]
    fn test_contains_with_vars() {
        let check = Check::Contains(Term::Var(0), Term::Var(1));
        let subst = Substitution::empty()
            .extend(0, Term::Const(symbol_from_str("read,write")))
            .unwrap()
            .extend(1, Term::Const(symbol_from_str("read")))
            .unwrap();
        assert!(eval_check(&check, &subst));
    }
}
