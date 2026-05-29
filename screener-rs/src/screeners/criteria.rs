use crate::screeners::models::ScreenRow;
use std::collections::BTreeSet;

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

fn cmp(left: Operand, op: CmpOp, right: Operand) -> Predicate {
    Predicate::Compare { left, op, right }
}

fn field(name: &'static str) -> Operand {
    Operand::Field(name)
}

fn val(value: f64) -> Operand {
    Operand::Value(value)
}

pub fn criteria_names() -> BTreeSet<&'static str> {
    [
        "ema",
        "breakout",
        "ema_breakout",
        "value",
        "quality",
        "cheap_quality",
        "undervalued",
        "dividend",
        "momentum_value",
        "intraday_momentum",
        "intraday_breakout",
        "near_52_high",
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
        "obv-trend",
        "vol-breakout",
    ]
    .into_iter()
    .collect()
}

pub fn predicates_for(name: &str) -> anyhow::Result<Vec<Predicate>> {
    let mut preds = match name {
        "ema" => vec![
            cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
            cmp(field("EMA20"), CmpOp::Gt, field("EMA100")),
            cmp(field("EMA100"), CmpOp::Gt, field("EMA200")),
            cmp(field("EMA200"), CmpOp::Gt, val(0.0)),
        ],
        "breakout" => vec![
            Predicate::AbovePct {
                field: "close",
                other: "price_52_week_high",
                pct: 0.9,
            },
            cmp(field("volume"), CmpOp::Gt, field("average_volume_10d_calc")),
        ],
        "value" => vec![
            cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Lte, val(20.0)),
        ],
        "quality" => vec![
            cmp(field("return_on_equity"), CmpOp::Gt, val(15.0)),
            cmp(field("debt_to_equity"), CmpOp::Lt, val(1.0)),
        ],
        "cheap_quality" => vec![
            cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Lte, val(20.0)),
            cmp(field("return_on_equity"), CmpOp::Gt, val(15.0)),
            cmp(field("debt_to_equity"), CmpOp::Lt, val(1.0)),
            cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
        ],
        "undervalued" => vec![
            cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Lte, val(12.0)),
            cmp(field("volume"), CmpOp::Gt, field("average_volume_10d_calc")),
        ],
        "dividend" => vec![
            cmp(field("dividend_yield_recent"), CmpOp::Gt, val(3.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Lte, val(25.0)),
            cmp(field("debt_to_equity"), CmpOp::Lt, val(1.5)),
        ],
        "momentum_value" => vec![
            cmp(field("price_earnings_ttm"), CmpOp::Gt, val(0.0)),
            cmp(field("price_earnings_ttm"), CmpOp::Lte, val(25.0)),
            cmp(field("RSI"), CmpOp::Gte, val(50.0)),
            cmp(field("RSI"), CmpOp::Lte, val(70.0)),
            cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
            cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
        ],
        "intraday_momentum" => vec![
            cmp(field("relative_volume_10d_calc"), CmpOp::Gte, val(1.5)),
            cmp(field("volume"), CmpOp::Gte, val(200_000.0)),
            cmp(field("close"), CmpOp::Gte, field("EMA20")),
            cmp(field("EMA20"), CmpOp::Gt, field("EMA200")),
            cmp(field("RSI"), CmpOp::Gte, val(55.0)),
            cmp(field("RSI"), CmpOp::Lte, val(80.0)),
            cmp(field("change"), CmpOp::Gte, val(1.0)),
        ],
        "intraday_breakout" => vec![
            Predicate::AbovePct {
                field: "close",
                other: "price_52_week_high",
                pct: 0.97,
            },
            cmp(field("relative_volume_10d_calc"), CmpOp::Gte, val(2.0)),
            cmp(field("change"), CmpOp::Gte, val(1.5)),
            cmp(field("EMA5"), CmpOp::Gt, field("EMA20")),
        ],
        "near_52_high" => vec![
            Predicate::BetweenPct {
                field: "close",
                other: "price_52_week_high",
                low: 0.8,
                high: 1.0,
            },
            cmp(field("close"), CmpOp::Lt, field("price_52_week_high")),
        ],
        "ema_breakout" => {
            let mut ema = predicates_for("ema")?;
            ema.extend(predicates_for("breakout")?);
            return Ok(ema);
        }
        _ => anyhow::bail!("unknown non-pipeline criterion {name:?}"),
    };
    preds.shrink_to_fit();
    Ok(preds)
}
