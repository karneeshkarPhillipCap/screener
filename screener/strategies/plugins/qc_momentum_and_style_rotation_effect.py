"""Momentum And Style Rotation Effect."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_style(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    closes = {sym: df["close"] for sym, df in ctx.bars_by_tv.items() if not df.empty}
    if not closes:
        return ctx.bars_by_tv

    close_df = pd.DataFrame(closes)

    # 12 month return
    ret_12m = close_df / close_df.shift(252) - 1.0

    rank = ret_12m.rank(axis=1, pct=True)
    # The max rank per day
    max_rank = rank.max(axis=1)
    min_rank = rank.min(axis=1)

    long_mask = rank.eq(max_rank, axis=0)
    short_mask = rank.eq(min_rank, axis=0)

    score = pd.DataFrame(0, index=close_df.index, columns=close_df.columns)
    score[long_mask] = 1.0
    score[short_mask] = -1.0

    # Rebalance start of next month
    monthly_score = score.resample("ME").last()
    daily_score = monthly_score.reindex(score.index).ffill()

    prepared = {}
    for sym, df in ctx.bars_by_tv.items():
        if sym in daily_score:
            df["qc_style_score"] = daily_score[sym]
        else:
            df["qc_style_score"] = 0.0
        prepared[sym] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_momentum_and_style_rotation_effect",
    entry="qc_style_score > 0",
    exit="qc_style_score <= 0",
    prepare_bars=_prepare_style,
    required_lookback=_lookback,
)
def _qc_style() -> None:
    pass
