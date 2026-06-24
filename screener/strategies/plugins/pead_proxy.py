"""Post-Earnings Announcement Drift (PEAD) Proxy."""

from __future__ import annotations

import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_pead(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()
        close = df["close"].astype(float)
        open_p = df["open"].astype(float)
        vol = df["volume"].astype(float)

        prev_close = close.shift(1)
        gap_up = (open_p / prev_close) > 1.05

        vol_sma20 = vol.rolling(20, min_periods=20).mean()
        vol_spike = vol > (vol_sma20 * 3.0)

        df["pead_entry"] = (gap_up & vol_spike).astype(float)
        prepared[symbol] = df
    return prepared


def _pead_lookback() -> int:
    return 20


@strategy(
    "pead_proxy",
    entry="pead_entry > 0",
    exit="close < sma(close, 10)",
    prepare_bars=_prepare_pead,
    required_lookback=_pead_lookback,
)
def _pead_proxy() -> None:
    """Expression-only strategy."""
