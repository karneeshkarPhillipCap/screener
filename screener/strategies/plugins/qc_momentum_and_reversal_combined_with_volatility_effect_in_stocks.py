"""Momentum And Reversal Combined With Volatility Effect In Stocks."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_mom_rev_vol(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Need cross-sectional ranking
    closes = {sym: df["close"] for sym, df in ctx.bars_by_tv.items() if not df.empty}
    if not closes:
        return ctx.bars_by_tv

    close_df = pd.DataFrame(closes)

    # 1-month lag
    shifted_close = close_df.shift(21)

    # 6-month return
    ret_6m = shifted_close / shifted_close.shift(126) - 1.0

    # 6-month volatility
    daily_ret = close_df.pct_change()
    shifted_ret = daily_ret.shift(21)
    vol_6m = shifted_ret.rolling(126).std() * np.sqrt(252)

    # Ranks
    vol_rank = vol_6m.rank(axis=1, pct=True)
    high_vol_mask = vol_rank >= 0.8

    high_vol_returns = ret_6m.where(high_vol_mask)
    ret_rank = high_vol_returns.rank(axis=1, pct=True)

    long_mask = high_vol_mask & (ret_rank >= 0.8)
    short_mask = high_vol_mask & (ret_rank <= 0.2)

    score = pd.DataFrame(0, index=close_df.index, columns=close_df.columns)
    score[long_mask] = 1.0
    score[short_mask] = -1.0

    # Monthly rebalance, 6 month holding = rolling 6 month sum of monthly signals
    monthly_score = score.resample("ME").last()
    held_score = monthly_score.rolling(6).sum()
    daily_held_score = held_score.reindex(score.index).ffill()

    prepared = {}
    for sym, df in ctx.bars_by_tv.items():
        if sym in daily_held_score:
            df["qc_mom_rev_vol_score"] = daily_held_score[sym]
        else:
            df["qc_mom_rev_vol_score"] = 0.0
        prepared[sym] = df

    return prepared


def _lookback() -> int:
    return 252 + 126


@strategy(
    "qc_momentum_and_reversal_combined_with_volatility_effect_in_stocks",
    entry="qc_mom_rev_vol_score > 0",
    exit="qc_mom_rev_vol_score <= 0",
    prepare_bars=_prepare_mom_rev_vol,
    required_lookback=_lookback,
)
def _qc_mom_rev_vol() -> None:
    pass
