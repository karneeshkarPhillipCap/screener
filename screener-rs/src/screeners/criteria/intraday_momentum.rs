use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "intraday_momentum";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("relative_volume_10d_calc"), CmpOp::Gte, val(1.5)),
        cmp(field("volume"), CmpOp::Gte, val(200_000.0)),
        cmp(field("close"), CmpOp::Gte, field("EMA20")),
        cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
        cmp(field("RSI"), CmpOp::Gte, val(55.0)),
        cmp(field("RSI"), CmpOp::Lte, val(80.0)),
        cmp(field("change"), CmpOp::Gte, val(1.0)),
    ]
}
