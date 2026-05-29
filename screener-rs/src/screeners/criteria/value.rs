use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "value";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Lte, val(20.0)),
    ]
}
