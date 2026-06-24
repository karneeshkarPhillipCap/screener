import pandas as pd
import numpy as np
import pandas_market_calendars as mcal
from screener.strategies.spec import strategy


@strategy("qc_pre_holiday_effect")
def pre_holiday_effect(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    Pre Holiday Effect.
    - Long SPY 2 days before a public holiday.
    - Cash otherwise.
    """
    weights = pd.DataFrame(0.0, index=prices.index, columns=prices.columns)
    if "SPY" not in prices.columns:
        return weights

    nyse = mcal.get_calendar("NYSE")
    # Get holidays
    holidays = nyse.holidays().holidays

    # Create a boolean series for days that are 1 or 2 trading days before a holiday
    # We can check if dt + 1 business day or dt + 2 business days falls on a holiday
    # Actually simpler: find all holidays, and for each, the 2 trading days prior are signal days.

    valid_dates = nyse.valid_days(
        start_date=prices.index[0], end_date=prices.index[-1]
    ).tz_localize(None)

    signal_dates = set()
    for hol in holidays:
        # find the index of the first valid day >= hol
        # and take the 2 valid days before it
        idx = valid_dates.searchsorted(np.datetime64(hol))
        if idx > 0:
            signal_dates.add(valid_dates[idx - 1].date())
        if idx > 1:
            signal_dates.add(valid_dates[idx - 2].date())

    for dt in prices.index:
        if dt.date() in signal_dates:
            weights.loc[dt, "SPY"] = 1.0

    return weights
