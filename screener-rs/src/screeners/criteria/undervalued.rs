use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "undervalued";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Lte, val(12.0)),
        cmp(field("volume"), CmpOp::Gt, field("average_volume_10d_calc")),
    ]
}
