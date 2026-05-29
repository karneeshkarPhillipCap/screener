use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "cheap_quality";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Lte, val(20.0)),
        cmp(field("return_on_equity"), CmpOp::Gt, val(15.0)),
        cmp(field("debt_to_equity"), CmpOp::Lt, val(1.0)),
        cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
    ]
}
