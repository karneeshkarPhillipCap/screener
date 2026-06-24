import pandas as pd
from screener.strategies.spec import strategy


@strategy("qc_lunar_cycle_in_equity_market")
def lunar_cycle(prices: pd.DataFrame, **kwargs) -> pd.DataFrame:
    """
    Lunar Cycle In Equity Market.
    - Long EEM 7 days before new moon.
    - Short EEM 7 days before full moon.
    - Synodic month = 29.530588 days.
    - Known new moon: 2000-01-06 18:14 UTC
    """
    weights = pd.DataFrame(0.0, index=prices.index, columns=prices.columns)

    if "EEM" not in prices.columns:
        return weights

    known_new_moon = pd.Timestamp("2000-01-06")
    synodic_month = 29.530588

    for dt in prices.index:
        days_since = (dt - known_new_moon).days
        phase = (days_since % synodic_month) / synodic_month

        # Phase 0.0 or 1.0 is New Moon.
        # Phase 0.5 is Full Moon.
        # 7 days is approx 7/29.53 = 0.237 phase difference.
        # 7 days before new moon: phase between 1.0 - 0.237 = 0.763 and 1.0
        # 7 days before full moon: phase between 0.5 - 0.237 = 0.263 and 0.5

        if 0.76 <= phase <= 1.0:
            weights.loc[dt, "EEM"] = 1.0
        elif 0.26 <= phase <= 0.5:
            weights.loc[dt, "EEM"] = -1.0

    return weights
