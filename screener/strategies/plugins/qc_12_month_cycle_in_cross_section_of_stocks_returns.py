import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_12_month_cycle_in_cross_section_of_stocks_returns")
def twelve_month_cycle(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    12 Month Cycle In Cross Section Of Stocks Returns.
    - Uses same calendar month return from one year ago.
    - Go long top decile, short bottom decile.
    - Monthly rebalance, equal weighted.
    """
    # 1-month return from 12 months ago
    # Return at t-12 = Price(t-11) / Price(t-12) - 1
    # We can approximate 1 month as 21 days, 12 months as 252 days.
    # Return from t-252 to t-231.
    monthly_prices = prices.resample("M").last()

    # 1-month return from 11 months ago (which corresponds to the 1-month period exactly 1 year prior to the next month)
    # Actually, return of month M in year Y-1 is (Price(Y-1, M) / Price(Y-1, M-1) - 1).
    returns_1m = monthly_prices.pct_change(1)

    # Shift by 11 months so that at the end of month t-1, we look at the return of month t in the previous year.
    # Wait, the rule is: "group according to performance in January one year ago" -> "same calendar month last year".
    signal = returns_1m.shift(11)

    ranks = signal.rank(axis=1, pct=True)

    weights = pd.DataFrame(
        0.0, index=monthly_prices.index, columns=monthly_prices.columns
    )

    for dt, row in ranks.iterrows():
        longs = row[row >= 0.9].index
        shorts = row[row <= 0.1].index

        if len(longs) > 0:
            weights.loc[dt, longs] = 1.0 / len(longs)
        if len(shorts) > 0:
            weights.loc[dt, shorts] = -1.0 / len(shorts)

    daily_weights = weights.reindex(prices.index, method="ffill")
    return daily_weights
