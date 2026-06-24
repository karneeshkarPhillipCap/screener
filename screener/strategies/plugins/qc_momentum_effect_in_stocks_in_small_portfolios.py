import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_momentum_effect_in_stocks_in_small_portfolios")
def momentum_effect_in_stocks_in_small_portfolios(
    prices: pd.DataFrame, **kwargs
) -> pd.DataFrame:
    """
    Momentum Effect In Stocks In Small Portfolios.
    - Exclude bottom 25% by market cap (here we just use universe provided, assuming small cap already filtered or we proxy with volume).
    - Ranked on yearly return.
    - Long top 10, Short bottom 10.
    - Equal weighted, yearly rebalance.
    """
    # Calculate 12-month (252-day) return
    returns_12m = prices.pct_change(252)

    # Resample to yearly (end of year)
    yearly_returns = returns_12m.resample("Y").last()

    # Rank cross-sectionally
    ranks = yearly_returns.rank(axis=1, ascending=False)

    weights = pd.DataFrame(
        0.0, index=yearly_returns.index, columns=yearly_returns.columns
    )

    # Top 10 long, bottom 10 short
    for dt, row in ranks.iterrows():
        top_10 = row[row <= 10].index
        bottom_10 = row.dropna().nlargest(10).index

        if len(top_10) > 0:
            weights.loc[dt, top_10] = 1.0 / len(top_10)
        if len(bottom_10) > 0:
            weights.loc[dt, bottom_10] = -1.0 / len(bottom_10)

    # Forward fill weights to daily
    daily_weights = weights.reindex(prices.index, method="ffill")
    return daily_weights
