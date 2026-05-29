use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "dividend";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("dividend_yield_recent"), CmpOp::Gt, val(3.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Lte, val(25.0)),
        cmp(field("debt_to_equity"), CmpOp::Lt, val(1.5)),
    ]
}
