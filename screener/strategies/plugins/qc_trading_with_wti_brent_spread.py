"""Trading With WTI Brent Spread."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_wti_brent(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # We approximate by computing spread of asset against benchmark
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    bench_close = benchmark_bars["close"].astype(float)

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        asset_close = df["close"].astype(float)

        # 1 yr rolling linear regression
        cov = asset_close.rolling(252).cov(bench_close)
        var = bench_close.rolling(252).var()
        beta = cov / var
        alpha = asset_close.rolling(252).mean() - beta * bench_close.rolling(252).mean()

        fair_value = (1 - beta) * asset_close - alpha
        spread = asset_close - bench_close
        sma_20 = spread.rolling(20).mean()

        df["qc_wti_brent_entry"] = spread < sma_20
        df["qc_wti_brent_exit"] = spread > fair_value
        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_trading_with_wti_brent_spread",
    entry="qc_wti_brent_entry",
    exit="qc_wti_brent_exit",
    prepare_bars=_prepare_wti_brent,
    required_lookback=_lookback,
)
def _qc_wti_brent() -> None:
    pass
