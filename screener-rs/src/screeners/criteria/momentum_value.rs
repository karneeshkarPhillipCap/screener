use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "momentum_value";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
        cmp(field("price_earnings_ttm"), CmpOp::Lte, val(25.0)),
        cmp(field("RSI"), CmpOp::Gte, val(50.0)),
        cmp(field("RSI"), CmpOp::Lte, val(70.0)),
        cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
        cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
    ]
}
