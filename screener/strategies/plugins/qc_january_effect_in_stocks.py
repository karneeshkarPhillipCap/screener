"""January Effect In Stocks."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_jan_eff(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        df["is_jan"] = pd.DatetimeIndex(df.index).month == 1
        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 10


@strategy(
    "qc_january_effect_in_stocks",
    entry="is_jan",
    exit="~is_jan",
    prepare_bars=_prepare_jan_eff,
    required_lookback=_lookback,
)
def _qc_jan_eff() -> None:
    pass
