"""Omni Momentum Strategy (Multi-Timeframe Clenow)."""

from __future__ import annotations

import pandas as pd
import numpy as np
import numpy.typing as npt

from screener.strategies.spec import PrepareCtx, strategy


def calc_clenow(y: pd.Series, N: int) -> npt.NDArray[np.float64]:
    if len(y) < N:
        return np.zeros(len(y))

    x = np.arange(N)
    x_mean = x.mean()
    x_diff = x - x_mean
    var_x = np.var(x)
    sum_x_diff_sq = np.sum(x_diff**2)

    w = x_diff[::-1]
    conv = np.convolve(y.to_numpy(), w, mode="valid")

    slope = np.full(len(y), np.nan)
    slope[N - 1 :] = conv / sum_x_diff_sq
    ann_slope = (np.exp(slope) ** 252) - 1.0

    cov = np.full(len(y), np.nan)
    cov[N - 1 :] = conv / N

    var_y = y.rolling(N).var(ddof=0).to_numpy()
    var_y_safe = np.where(var_y == 0, np.nan, var_y)

    r2 = (cov**2) / (var_x * var_y_safe)

    score = ann_slope * r2
    return np.asarray(np.nan_to_num(score), dtype=np.float64)


def _prepare_omni(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    benchmark_close = benchmark_bars["close"].astype(float)
    bench_sma100 = benchmark_close.rolling(100, min_periods=100).mean()
    bench_regime = benchmark_close > bench_sma100

    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)
        y = pd.Series(np.log(close.to_numpy()), index=close.index)

        c45 = calc_clenow(y, 45)
        c90 = calc_clenow(y, 90)
        c180 = calc_clenow(y, 180)

        omni_score = (c45 + c90 + c180) / 3.0

        # Stock-specific trend filter
        stock_sma100 = close.rolling(100, min_periods=100).mean()
        stock_regime = close > stock_sma100

        # Benchmark Regime Alignment
        regime = bench_regime.reindex(df.index).fillna(False)

        df["omni_score"] = np.where(
            regime & stock_regime & (omni_score > 0), omni_score, 0.0
        )
        prepared[symbol] = df

    return prepared


def _omni_lookback() -> int:
    return 200


@strategy(
    "omni_momentum",
    entry="omni_score > 0.0",
    exit="omni_score <= 0",
    prepare_bars=_prepare_omni,
    required_lookback=_omni_lookback,
)
def _omni_momentum() -> None:
    """Expression-only strategy. Body unused."""
