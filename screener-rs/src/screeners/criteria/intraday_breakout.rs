use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "intraday_breakout";

pub fn predicates() -> Vec<Predicate> {
    vec![
        Predicate::AbovePct {
            field: "close",
            other: "price_52_week_high",
            pct: 0.97,
        },
        cmp(field("relative_volume_10d_calc"), CmpOp::Gte, val(2.0)),
        cmp(field("change"), CmpOp::Gte, val(1.5)),
        cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
    ]
}
