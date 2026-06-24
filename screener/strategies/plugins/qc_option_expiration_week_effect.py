"""Option Expiration Week Effect."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_option(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()

        # Third Friday logic
        # 1. find all Fridays
        fridays = df[df.index.weekday == 4].index
        # 2. group by year and month
        # A day is a third Friday if 15 <= day <= 21
        third_fridays = fridays[(fridays.day >= 15) & (fridays.day <= 21)]

        # We long on Monday of the expiration week. So from that Monday (third_fridays - 4 days) to Friday.
        # Actually it's simpler: just mark days where the NEXT friday is a third friday and within 4 days.
        # Let's just create a dummy for the week.

        df["qc_option_week"] = False

        for tf in third_fridays:
            # Mark the 5 days before it (Mon to Fri)
            start_date = tf - pd.Timedelta(days=4)
            df.loc[start_date:tf, "qc_option_week"] = True

        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 30


@strategy(
    "qc_option_expiration_week_effect",
    entry="qc_option_week",
    exit="~qc_option_week",
    prepare_bars=_prepare_option,
    required_lookback=_lookback,
)
def _qc_option() -> None:
    pass
