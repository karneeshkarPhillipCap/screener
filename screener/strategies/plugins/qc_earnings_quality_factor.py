"""Earnings Quality Factor."""

from __future__ import annotations

from typing import Any

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_eq(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Proxy: Smooth trend (R^2 of log prices) * Return
    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        # Return 252
        ret = close / close.shift(252) - 1.0

        # R^2 of log prices
        log_p = pd.Series(np.log(close.to_numpy()), index=close.index)
        x = np.arange(252)
        x_var = np.var(x)

        # rolling cov(x, y)
        def r2(y: np.ndarray[Any, Any]) -> float:
            if len(y) < 252 or np.isnan(y).any():
                return 0.0
            cov = np.cov(x, y)[0, 1]
            beta = cov / x_var
            y_pred = beta * x + np.mean(y) - beta * np.mean(x)
            ss_tot = np.sum((y - np.mean(y)) ** 2)
            ss_res = np.sum((y - y_pred) ** 2)
            return 1 - (ss_res / ss_tot) if ss_tot != 0 else 0.0

        # compute over every window (slow, so sample monthly)
        r2_series = pd.Series(0.0, index=df.index)
        month_ends = df.resample("ME").last().index
        index = pd.DatetimeIndex(df.index)
        for me in month_ends:
            idx = index.get_indexer(pd.Index([me]), method="pad")[0]
            if idx >= 252:
                r2_series.iloc[idx] = r2(
                    log_p.iloc[idx - 251 : idx + 1].to_numpy(dtype=np.float64)
                )

        r2_series = r2_series.replace(0.0, np.nan).ffill()
        df["qc_eq_score"] = r2_series * ret
        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_earnings_quality_factor",
    entry="qc_eq_score > 0",
    exit="qc_eq_score < 0",
    prepare_bars=_prepare_eq,
    required_lookback=_lookback,
)
def _qc_eq() -> None:
    pass
