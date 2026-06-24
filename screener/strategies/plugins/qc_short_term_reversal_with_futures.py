import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_short_term_reversal_with_futures")
def short_term_reversal_futures(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    Short Term Reversal With Futures.
    - Weekly reversal.
    - We proxy futures with ETFs.
    - We skip Open Interest and just select the top 50% by volume.
    """
    weights = pd.DataFrame(0.0, index=prices.index, columns=prices.columns)

    # We don't have volume in the pure prices dataframe standard signature.
    # Gap: Without volume, we just do pure short term reversal on all assets.
    # Weekly prices (Wednesdays)
    weekly_prices = prices.resample("W-WED").last()

    returns_1w = weekly_prices.pct_change(1)

    for dt, row in returns_1w.iterrows():
        valid_returns = row.dropna()
        if len(valid_returns) < 2:
            continue

        # Lowest return -> Long, Highest return -> Short
        lowest = valid_returns.idxmin()
        highest = valid_returns.idxmax()

        weights.loc[dt, lowest] = 1.0
        weights.loc[dt, highest] = -1.0

    daily_weights = weights.reindex(prices.index, method="ffill")
    return daily_weights
