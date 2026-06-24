"""Momentum Effect In Commodities Futures strategy implementation."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_commodities_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximation of Momentum Effect in Commodities Futures.
    Calculates 12-month (252 days) ROC, ranks cross-sectionally at month end.
    Longs top 25%, Shorts bottom 25%.
    """
    scores = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
        close = bars["close"].astype(float)
        roc252 = close.pct_change(252)
        scores[sym] = roc252

    if not scores:
        return ctx.bars_by_tv

    df_scores = pd.DataFrame(scores)

    # Monthly rebalance
    df_scores_monthly = df_scores.resample("ME").last()

    # Rank cross-sectionally
    ranks_monthly = df_scores_monthly.rank(axis=1, pct=True)

    top_25_pct_monthly = ranks_monthly >= 0.75
    bottom_25_pct_monthly = ranks_monthly <= 0.25

    # Reindex back to daily, forward fill the monthly selection
    top_25_pct = top_25_pct_monthly.reindex(df_scores.index, method="ffill").fillna(
        False
    )
    bottom_25_pct = bottom_25_pct_monthly.reindex(
        df_scores.index, method="ffill"
    ).fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Shift signals by 1 to avoid lookahead
        is_long = (
            top_25_pct.get(sym, pd.Series(False, index=df.index)).shift(1).fillna(False)
        )
        is_short = (
            bottom_25_pct.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
        )

        signal = pd.Series(0, index=df.index)
        signal[is_long] = 1
        signal[is_short] = -1

        df["momentum_signal"] = signal.astype(int)
        prepared[sym] = df

    return prepared


def _commodities_momentum_lookback() -> int:
    return 252


@strategy(
    "qc_momentum-effect-in-commodities-futures",
    entry="momentum_signal == 1",
    exit="momentum_signal == 0",
    prepare_bars=_prepare_commodities_momentum,
    required_lookback=_commodities_momentum_lookback,
)
def _qc_commodities_momentum() -> None:
    """Expression-only strategy."""
