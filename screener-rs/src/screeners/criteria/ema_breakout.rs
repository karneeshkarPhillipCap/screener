use super::{Predicate, breakout, ema_predicates};

pub const NAME: &str = "ema_breakout";

pub fn predicates() -> Vec<Predicate> {
    let mut predicates = ema_predicates();
    predicates.extend(breakout::predicates());
    predicates
}
