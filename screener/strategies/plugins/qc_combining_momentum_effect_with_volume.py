import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_combining_momentum_effect_with_volume")
def momentum_with_volume(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    Combining Momentum Effect With Volume.
    - Requires Volume to compute turnover.
    - We approximate by just doing Momentum if volume is missing, or we assume a `volume` df is passed in kwargs.
    """
    volume = kwargs.get("volume", None)

    monthly_prices = prices.resample("M").last()
    returns_12m = monthly_prices.pct_change(12)

    weights = pd.DataFrame(
        0.0, index=monthly_prices.index, columns=monthly_prices.columns
    )

    for dt, row in returns_12m.iterrows():
        valid = row.dropna()
        if len(valid) < 10:
            continue

        # Top decile momentum
        q10 = valid.quantile(0.9)
        q1 = valid.quantile(0.1)

        top_mom = valid[valid >= q10].index
        bot_mom = valid[valid <= q1].index

        # If we have volume, we find highest volume among top_mom and bot_mom
        if volume is not None and dt in volume.index:
            vol_row = volume.loc[dt]
            if len(top_mom) > 0:
                long_asset = vol_row[top_mom].idxmax()
                weights.loc[dt, long_asset] += 1.0
            if len(bot_mom) > 0:
                short_asset = vol_row[bot_mom].idxmax()
                weights.loc[dt, short_asset] -= 1.0
        else:
            # Fallback: equal weight top momentum
            if len(top_mom) > 0:
                weights.loc[dt, top_mom] = 1.0 / len(top_mom)
            if len(bot_mom) > 0:
                weights.loc[dt, bot_mom] = -1.0 / len(bot_mom)

    # 3-month holding period (rolling average of weights over 3 months)
    smoothed_weights = weights.rolling(3).mean()
    daily_weights = smoothed_weights.reindex(prices.index, method="ffill")
    return daily_weights
