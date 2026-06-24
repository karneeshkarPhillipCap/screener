import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_capm_alpha(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate CAPM alpha over 21 days and select top 2."""
    lookback = 21

    # Extract daily returns
    returns = pd.DataFrame(
        {sym: df["close"].pct_change() for sym, df in ctx.bars_by_tv.items()}
    )

    # Determine benchmark
    if "SPY" in returns.columns:
        benchmark_ret = returns["SPY"]
    else:
        # Approximation: equal-weighted market return of the universe
        benchmark_ret = returns.mean(axis=1)

    # Variance and mean of the benchmark
    var_m = benchmark_ret.rolling(window=lookback).var()
    mean_m = benchmark_ret.rolling(window=lookback).mean()

    alpha_panel = pd.DataFrame(index=returns.index, columns=returns.columns)

    for sym in returns.columns:
        # Sample covariance between asset and benchmark
        cov_im = returns[sym].rolling(window=lookback).cov(benchmark_ret)
        mean_i = returns[sym].rolling(window=lookback).mean()

        # Calculate beta and alpha (standard OLS formulas)
        beta_i = cov_im / var_m
        alpha_i = mean_i - beta_i * mean_m
        alpha_panel[sym] = alpha_i

    # Rank daily across symbols: top 2 have highest alpha
    ranks = alpha_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if ranked 1 or 2
        df["signal"] = (ranks[sym] <= 2).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_capm_alpha_ranking_strategy_on_dow_30_companies",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_capm_alpha,
    required_lookback=lambda: (
        22
    ),  # Need 21 periods for rolling stats + 1 for pct_change
)
def _qc_capm_alpha_ranking_strategy_on_dow_30_companies():
    pass
