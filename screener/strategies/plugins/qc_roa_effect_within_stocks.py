import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_roa_effect_within_stocks")
def roa_effect(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    ROA Effect Within Stocks.
    - Proxy ROA (Quality) using 1 / volatility if ROA fundamental data is missing.
    - Monthly rebalance.
    """
    monthly_prices = prices.resample("M").last()
    returns = prices.pct_change()

    weights = pd.DataFrame(
        0.0, index=monthly_prices.index, columns=monthly_prices.columns
    )

    for dt in monthly_prices.index:
        start_dt = dt - pd.DateOffset(years=1)
        window_ret = returns.loc[start_dt:dt]

        if len(window_ret) < 100:
            continue

        vol = window_ret.std()
        proxy_roa = 1.0 / vol  # Low volatility as proxy for high quality/ROA

        proxy_roa = proxy_roa.dropna()
        if len(proxy_roa) < 10:
            continue

        q10 = proxy_roa.quantile(0.9)
        q1 = proxy_roa.quantile(0.1)

        longs = proxy_roa[proxy_roa >= q10].index
        shorts = proxy_roa[proxy_roa <= q1].index

        if len(longs) > 0:
            weights.loc[dt, longs] = 1.0 / len(longs)
        if len(shorts) > 0:
            weights.loc[dt, shorts] = -1.0 / len(shorts)

    daily_weights = weights.reindex(prices.index, method="ffill")
    return daily_weights
