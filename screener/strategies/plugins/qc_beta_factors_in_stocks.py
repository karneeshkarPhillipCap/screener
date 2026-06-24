import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_beta_factors_in_stocks")
def beta_factors(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    Beta Factors In Stocks.
    - Long bottom 5 beta stocks, short top 5 beta stocks.
    - Monthly rebalance.
    """
    returns = prices.pct_change()

    # Proxies SPY as the market. If SPY not in columns, use equally weighted universe as market.
    if "SPY" in returns.columns:
        market_ret = returns["SPY"]
    else:
        market_ret = returns.mean(axis=1)

    monthly_prices = prices.resample("M").last()
    weights = pd.DataFrame(
        0.0, index=monthly_prices.index, columns=monthly_prices.columns
    )

    for i in range(12, len(monthly_prices)):
        dt = monthly_prices.index[i]
        start_dt = dt - pd.DateOffset(years=1)

        # 1-year window
        window_ret = returns.loc[start_dt:dt]
        if len(window_ret) < 100:
            continue

        mkt_var = (
            window_ret.mean(axis=1).var()
            if "SPY" not in returns.columns
            else window_ret["SPY"].var()
        )
        if mkt_var == 0:
            continue

        betas = window_ret.cov().loc[
            window_ret.columns,
            "SPY" if "SPY" in window_ret.columns else window_ret.columns[0],
        ]
        # Actually a better way to do beta against market:
        cov_with_mkt = window_ret.apply(
            lambda x: x.cov(market_ret.loc[window_ret.index])
        )
        betas = cov_with_mkt / market_ret.loc[window_ret.index].var()

        # Exclude market proxy itself if it's an ETF like SPY
        if "SPY" in betas.index:
            betas = betas.drop("SPY")

        betas = betas.dropna()
        if len(betas) < 10:
            continue

        lowest_5 = betas.nsmallest(5).index
        highest_5 = betas.nlargest(5).index

        # Inverse beta weighting for low beta, direct for high beta, or just equal weight.
        # Paper says: lower-beta has larger weight in low-beta portfolio.
        # Let's use equal weights for simplicity as a defensible approximation.
        weights.loc[dt, lowest_5] = 1.0 / 5
        weights.loc[dt, highest_5] = -1.0 / 5

    daily_weights = weights.reindex(prices.index, method="ffill")
    return daily_weights
