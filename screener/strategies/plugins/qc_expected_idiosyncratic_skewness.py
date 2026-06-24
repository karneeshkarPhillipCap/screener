import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_idio_skewness(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximates Expected Idiosyncratic Skewness using historical idiosyncratic skewness
    over a 21-day rolling window relative to the benchmark.
    Longs the bottom 5% (lowest skewness).
    """
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark)
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    bench_close = benchmark_bars["close"].astype(float)
    bench_ret_1d = bench_close.pct_change(1)
    bench_var_21d = bench_ret_1d.rolling(21, min_periods=21).var()

    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 21:
            df["signal"] = 0
            continue

        close = df["close"].astype(float)
        ret_1d = close.pct_change(1)

        bret_1d = bench_ret_1d.reindex(df.index)
        bvar_21d = bench_var_21d.reindex(df.index)

        cov_21d = ret_1d.rolling(21, min_periods=21).cov(bret_1d)
        beta = cov_21d / bvar_21d

        # Calculate daily residual
        residual = ret_1d - beta * bret_1d

        # Calculate skewness of the residual over 21 days
        skew_21d = residual.rolling(21, min_periods=21).skew()

        df["idio_skew"] = skew_21d

    # Cross-sectional ranking
    skew_panel = pd.DataFrame(
        {
            sym: df.get("idio_skew", pd.Series(np.nan, index=df.index))
            for sym, df in ctx.bars_by_tv.items()
        }
    )

    # Ranks (1 is lowest skewness)
    ranks = skew_panel.rank(axis=1, ascending=True, pct=True)

    for sym, df in ctx.bars_by_tv.items():
        pct_rank = ranks[sym]
        # Bottom 5% of skewness (meaning highly negative skew) gets the long signal
        signal = np.where(pct_rank <= 0.05, 1, 0)
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_expected_idiosyncratic_skewness",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_idio_skewness,
    required_lookback=lambda: 21,
)
def _qc_expected_idiosyncratic_skewness():
    pass
