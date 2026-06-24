"""VIX Predicts Stock Index Returns strategy."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_vix_predict(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    If the VIX index is extremely high (>90th percentile over the last 2 years), we go long.
    If it is extremely low (<10th percentile), we go short.
    If VIX is not provided in the price panel, we use the asset's own 21-day realized
    volatility as a localized 'fear index' proxy.
    """
    vix_df = None
    for ticker in ["^VIX", "VIX", "VIXY"]:
        if (
            ticker in ctx.price_panel
            and ctx.price_panel[ticker] is not None
            and not ctx.price_panel[ticker].empty
        ):
            vix_df = ctx.price_panel[ticker]
            break

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy().sort_index()

        if vix_df is not None:
            vix_close = vix_df["close"].reindex(df.index, method="ffill")
        else:
            # Fallback: annualized 21-day historical volatility
            vix_close = df["close"].pct_change().rolling(21).std() * np.sqrt(252)

        # 2-year rolling rank (approx 504 trading days)
        rolling_rank = vix_close.rolling(504, min_periods=252).rank(pct=True)

        df["vix_long"] = (rolling_rank > 0.90).astype(int).shift(1).fillna(0)
        df["vix_short"] = (rolling_rank < 0.10).astype(int).shift(1).fillna(0)
        df["vix_signal"] = df["vix_long"] - df["vix_short"]

        prepared[sym] = df

    return prepared


def _vix_lookback() -> int:
    return 504


@strategy(
    "qc_vix-predicts-stock-index-returns",
    entry="vix_signal > 0",
    exit="vix_signal == 0",
    prepare_bars=_prepare_vix_predict,
    required_lookback=_vix_lookback,
)
def _qc_vix_predict() -> None:
    """Expression-only strategy."""
