"""January Barometer Strategy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_jan_baro(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()

        # We only trade SPY, but we can apply the logic universally
        index = pd.DatetimeIndex(df.index)
        df["is_jan"] = index.month == 1

        # Calculate monthly returns
        monthly = df["close"].resample("ME").last()
        monthly_ret = monthly.pct_change()

        # Get Jan returns per year
        monthly_index = pd.DatetimeIndex(monthly_ret.index)
        jan_ret = monthly_ret[monthly_index.month == 1]
        bull_years = set(pd.DatetimeIndex(jan_ret[jan_ret > 0].index).year)

        df["is_bull_year"] = index.year.isin(bull_years)
        df["qc_jan_baro_signal"] = df["is_jan"] | df["is_bull_year"]

        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_january_barometer",
    entry="qc_jan_baro_signal",
    exit="~qc_jan_baro_signal",
    prepare_bars=_prepare_jan_baro,
    required_lookback=_lookback,
)
def _qc_january_barometer() -> None:
    pass
