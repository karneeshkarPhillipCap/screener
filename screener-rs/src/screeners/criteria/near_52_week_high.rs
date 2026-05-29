use super::{CmpOp, Predicate, cmp, field};

pub const NAME: &str = "near_52_high";

pub fn predicates() -> Vec<Predicate> {
    vec![
        Predicate::BetweenPct {
            field: "close",
            other: "price_52_week_high",
            low: 0.8,
            high: 1.0,
        },
        cmp(field("close"), CmpOp::Lt, field("price_52_week_high")),
    ]
}
