"""Standardized Unexpected Earnings (SUE) strategy implementation."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_sue(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Since we do not have quarterly fundamental EPS data in this environment,
    we approximate SUE using Standardized Unexpected Price Jump (SUPJ) over 1 quarter (63 days).
    SUPJ = (ROC(63) - SMA(ROC(63), 252)) / STD(ROC(63), 252)
    We then rank stocks cross-sectionally at the end of each month and go long the top 5%.
    """
    scores = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
        close = bars["close"].astype(float)
        roc63 = close.pct_change(63)
        mean_roc = roc63.rolling(252, min_periods=63).mean()
        std_roc = roc63.rolling(252, min_periods=63).std()
        score = (roc63 - mean_roc) / (std_roc + 1e-8)
        scores[sym] = score

    if not scores:
        return ctx.bars_by_tv

    # Align into a single DataFrame
    df_scores = pd.DataFrame(scores)

    # Monthly rebalance: resample to end of month, rank, and find top 5%
    df_scores_monthly = df_scores.resample("ME").last()
    ranks_monthly = df_scores_monthly.rank(axis=1, pct=True)
    top_5_pct_monthly = ranks_monthly >= 0.95

    # Reindex back to daily, forward fill the monthly selection
    top_5_pct = top_5_pct_monthly.reindex(df_scores.index, method="ffill").fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Ensure we don't look ahead: shift the monthly signal by 1 day so we enter on the day after rebalance
        df["sue_signal"] = (
            top_5_pct.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        prepared[sym] = df

    return prepared


def _sue_lookback() -> int:
    # 63 days for return, 252 days for mean/std
    return 63 + 252


@strategy(
    "qc_standardized-unexpected-earnings",
    entry="sue_signal > 0",
    exit="sue_signal == 0",
    prepare_bars=_prepare_sue,
    required_lookback=_sue_lookback,
)
def _qc_sue() -> None:
    """Expression-only strategy."""
