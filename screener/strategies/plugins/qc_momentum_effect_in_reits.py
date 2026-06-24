"""Momentum Effect In REITs."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_reit(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    closes = {sym: df["close"] for sym, df in ctx.bars_by_tv.items() if not df.empty}
    if not closes:
        return ctx.bars_by_tv

    close_df = pd.DataFrame(closes)

    # 11-month return one-month lagged
    shifted_close = close_df.shift(21)
    ret_11m = shifted_close / shifted_close.shift(231) - 1.0

    # Rank
    rank = ret_11m.rank(axis=1, pct=True)
    top_tercile = rank >= 0.66

    score = pd.DataFrame(0, index=close_df.index, columns=close_df.columns)
    score[top_tercile] = 1.0

    # held for 3 months, rebalance quarterly -> rolling sum of monthly score
    monthly_score = score.resample("ME").last()
    held_score = monthly_score.rolling(3).sum()
    daily_held_score = held_score.reindex(score.index).ffill()

    prepared = {}
    for sym, df in ctx.bars_by_tv.items():
        if sym in daily_held_score:
            df["qc_reit_mom_score"] = daily_held_score[sym]
        else:
            df["qc_reit_mom_score"] = 0.0
        prepared[sym] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_momentum_effect_in_reits",
    entry="qc_reit_mom_score > 0",
    exit="qc_reit_mom_score <= 0",
    prepare_bars=_prepare_reit,
    required_lookback=_lookback,
)
def _qc_reit() -> None:
    pass
