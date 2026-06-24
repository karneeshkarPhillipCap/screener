import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_residual_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximates Residual Momentum by using CAPM (single market factor).
    Calculates 3-year (756 days) rolling beta.
    Calculates 1-year (252 days) residual return (Asset Return 1Y - Beta * Benchmark Return 1Y).
    Divides by 1-year idiosyncratic volatility.
    Long top 10%, Short bottom 10%.
    """
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark)
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    bench_close = benchmark_bars["close"].astype(float)
    bench_ret_1d = bench_close.pct_change(1)
    bench_ret_1y = bench_close.pct_change(252)
    bench_var_3y = bench_ret_1d.rolling(756, min_periods=252).var()

    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 252:
            df["signal"] = 0
            continue

        close = df["close"].astype(float)
        ret_1d = close.pct_change(1)
        ret_1y = close.pct_change(252)

        # Align benchmark returns
        bret_1d = bench_ret_1d.reindex(df.index)
        bret_1y = bench_ret_1y.reindex(df.index)
        bvar_3y = bench_var_3y.reindex(df.index)

        # Calculate rolling 3Y beta
        cov_3y = ret_1d.rolling(756, min_periods=252).cov(bret_1d)
        beta = cov_3y / bvar_3y

        # Calculate 1Y residual return
        residual_ret_1y = ret_1y - beta * bret_1y

        # Calculate idiosyncratic volatility (1Y)
        daily_residual = ret_1d - beta * bret_1d
        idio_vol_1y = daily_residual.rolling(252, min_periods=126).std()

        # Residual momentum score
        df["resid_mom_score"] = residual_ret_1y / idio_vol_1y

    # Cross-sectional ranking
    score_panel = pd.DataFrame(
        {
            sym: df.get("resid_mom_score", pd.Series(np.nan, index=df.index))
            for sym, df in ctx.bars_by_tv.items()
        }
    )

    # Ranks (1 is highest score)
    ranks = score_panel.rank(axis=1, ascending=False, pct=True)

    for sym, df in ctx.bars_by_tv.items():
        # Top 10% get 1, Bottom 10% get -1
        # ranks are percentage. Top 10% means pct <= 0.10. Bottom 10% means pct >= 0.90
        pct_rank = ranks[sym]
        signal = np.where(pct_rank <= 0.10, 1, np.where(pct_rank >= 0.90, -1, 0))
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_residual_momentum",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_residual_momentum,
    required_lookback=lambda: 756,
)
def _qc_residual_momentum():
    pass
