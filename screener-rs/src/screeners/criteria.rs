use crate::screeners::models::ScreenRow;
use std::collections::BTreeSet;

pub mod breakout;
pub mod cheap_quality;
pub mod dividend;
pub mod ema_breakout;
pub mod intraday_breakout;
pub mod intraday_momentum;
pub mod momentum_value;
pub mod near_52_week_high;
pub mod obv_trend;
pub mod quality;
pub mod undervalued;
pub mod value;
pub mod vol_breakout;

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Field(&'static str),
    Value(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Compare {
        left: Operand,
        op: CmpOp,
        right: Operand,
    },
    AbovePct {
        field: &'static str,
        other: &'static str,
        pct: f64,
    },
    BetweenPct {
        field: &'static str,
        other: &'static str,
        low: f64,
        high: f64,
    },
}

impl Predicate {
    pub fn evaluate(&self, row: &ScreenRow) -> bool {
        match self {
            Predicate::Compare { left, op, right } => {
                let Some(left) = value(left, row) else {
                    return false;
                };
                let Some(right) = value(right, row) else {
                    return false;
                };
                match op {
                    CmpOp::Gt => left > right,
                    CmpOp::Gte => left >= right,
                    CmpOp::Lt => left < right,
                    CmpOp::Lte => left <= right,
                    CmpOp::Eq => (left - right).abs() < f64::EPSILON,
                }
            }
            Predicate::AbovePct { field, other, pct } => {
                let Some(left) = row.numeric(field) else {
                    return false;
                };
                let Some(right) = row.numeric(other) else {
                    return false;
                };
                left >= right * pct
            }
            Predicate::BetweenPct {
                field,
                other,
                low,
                high,
            } => {
                let Some(left) = row.numeric(field) else {
                    return false;
                };
                let Some(right) = row.numeric(other) else {
                    return false;
                };
                left >= right * low && left <= right * high
            }
        }
    }

    pub fn fields(&self) -> Vec<&'static str> {
        match self {
            Predicate::Compare { left, right, .. } => {
                let mut out = Vec::new();
                if let Operand::Field(field) = left {
                    out.push(*field);
                }
                if let Operand::Field(field) = right {
                    out.push(*field);
                }
                out
            }
            Predicate::AbovePct { field, other, .. }
            | Predicate::BetweenPct { field, other, .. } => vec![*field, *other],
        }
    }
}

fn value(op: &Operand, row: &ScreenRow) -> Option<f64> {
    match op {
        Operand::Field(field) => row.numeric(field),
        Operand::Value(value) => Some(*value),
    }
}

pub(super) fn cmp(left: Operand, op: CmpOp, right: Operand) -> Predicate {
    Predicate::Compare { left, op, right }
}

pub(super) fn field(name: &'static str) -> Operand {
    Operand::Field(name)
}

pub(super) fn val(value: f64) -> Operand {
    Operand::Value(value)
}

pub(super) fn ema_predicates() -> Vec<Predicate> {
    vec![
        cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
        cmp(field("EMA20"), CmpOp::Gt, field("EMA100")),
        cmp(field("EMA100"), CmpOp::Gt, field("EMA200")),
        cmp(field("EMA200"), CmpOp::Gt, val(0.0)),
    ]
}

pub fn criteria_names() -> BTreeSet<&'static str> {
    [
        "ema",
        breakout::NAME,
        ema_breakout::NAME,
        value::NAME,
        quality::NAME,
        cheap_quality::NAME,
        undervalued::NAME,
        dividend::NAME,
        momentum_value::NAME,
        intraday_momentum::NAME,
        intraday_breakout::NAME,
        near_52_week_high::NAME,
    ]
    .into_iter()
    .collect()
}

pub fn pipeline_names() -> BTreeSet<&'static str> {
    [
        "garp",
        "promoter-buys",
        "rs-breakout",
        "unusual-volume",
        obv_trend::NAME,
        vol_breakout::NAME,
    ]
    .into_iter()
    .collect()
}

pub fn predicates_for(name: &str) -> anyhow::Result<Vec<Predicate>> {
    let mut preds = match name {
        "ema" => ema_predicates(),
        breakout::NAME => breakout::predicates(),
        value::NAME => value::predicates(),
        quality::NAME => quality::predicates(),
        cheap_quality::NAME => cheap_quality::predicates(),
        undervalued::NAME => undervalued::predicates(),
        dividend::NAME => dividend::predicates(),
        momentum_value::NAME => momentum_value::predicates(),
        intraday_momentum::NAME => intraday_momentum::predicates(),
        intraday_breakout::NAME => intraday_breakout::predicates(),
        near_52_week_high::NAME => near_52_week_high::predicates(),
        ema_breakout::NAME => ema_breakout::predicates(),
        _ => anyhow::bail!("unknown non-pipeline criterion {name:?}"),
    };
    preds.shrink_to_fit();
    Ok(preds)
}
