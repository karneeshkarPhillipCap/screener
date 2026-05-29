use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SlippageModel {
    Fixed {
        bps: f64,
    },
    HalfSpread {
        half_spread_bps: f64,
    },
    VolumeImpact {
        k: f64,
    },
    Composite {
        fixed_bps: f64,
        half_spread_bps: f64,
        vol_impact_k: f64,
    },
}

impl Default for SlippageModel {
    fn default() -> Self {
        Self::Fixed { bps: 0.0 }
    }
}

impl SlippageModel {
    pub fn adverse_fraction(&self, shares: f64, adv: f64, sigma_daily: f64) -> f64 {
        match *self {
            SlippageModel::Fixed { bps } => bps / 10_000.0,
            SlippageModel::HalfSpread { half_spread_bps } => half_spread_bps / 10_000.0,
            SlippageModel::VolumeImpact { k } => volume_impact(k, shares, adv, sigma_daily),
            SlippageModel::Composite {
                fixed_bps,
                half_spread_bps,
                vol_impact_k,
            } => {
                fixed_bps / 10_000.0
                    + half_spread_bps / 10_000.0
                    + volume_impact(vol_impact_k, shares, adv, sigma_daily)
            }
        }
    }
}

fn volume_impact(k: f64, shares: f64, adv: f64, sigma_daily: f64) -> f64 {
    if adv <= 0.0 || shares <= 0.0 || sigma_daily <= 0.0 {
        return 0.0;
    }
    k * sigma_daily * (shares / adv).sqrt()
}

pub fn apply_slippage(
    model: &SlippageModel,
    reference_price: f64,
    side: Side,
    shares: f64,
    adv: f64,
    sigma_daily: f64,
) -> f64 {
    let frac = model.adverse_fraction(shares, adv, sigma_daily).max(0.0);
    match side {
        Side::Buy => reference_price * (1.0 + frac),
        Side::Sell => reference_price * (1.0 - frac),
    }
}
