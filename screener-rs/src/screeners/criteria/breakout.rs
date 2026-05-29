use super::{CmpOp, Predicate, cmp, field};

pub const NAME: &str = "breakout";

pub fn predicates() -> Vec<Predicate> {
    vec![
        Predicate::AbovePct {
            field: "close",
            other: "price_52_week_high",
            pct: 0.9,
        },
        cmp(field("volume"), CmpOp::Gt, field("average_volume_10d_calc")),
    ]
}
