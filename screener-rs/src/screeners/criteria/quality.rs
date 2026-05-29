use super::{CmpOp, Predicate, cmp, field, val};

pub const NAME: &str = "quality";

pub fn predicates() -> Vec<Predicate> {
    vec![
        cmp(field("return_on_equity"), CmpOp::Gt, val(15.0)),
        cmp(field("debt_to_equity"), CmpOp::Lt, val(1.0)),
    ]
}
