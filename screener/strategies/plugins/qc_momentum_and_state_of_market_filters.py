import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_momentum_state(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Market state
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        bench_regime = pd.Series(True, index=pd.DatetimeIndex([]))
    else:
        benchmark_close = benchmark_bars["close"].astype(float)
        roc_252 = benchmark_close / benchmark_close.shift(252) - 1.0
        bench_regime = roc_252 > 0

    # Collect closes
    closes = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[symbol] = bars["close"].astype(float)

    if not closes:
        return ctx.bars_by_tv

    close_df = pd.DataFrame(closes)

    # Calculate 6-month momentum (126 days)
    mom_126 = close_df / close_df.shift(126) - 1.0

    # Rank cross-sectionally (1 is highest momentum)
    ranks = mom_126.rank(axis=1, ascending=False)

    # We want top 20
    top_20 = ranks <= 20

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy()

        regime = bench_regime.reindex(df.index).fillna(False)
        is_top = top_20[symbol].reindex(df.index).fillna(False)

        # We only enter if regime is UP and we are in top 20
        df["mom_state_entry"] = regime & is_top
        df["mom_state_exit"] = ~df["mom_state_entry"]
        prepared[symbol] = df

    return prepared


def _mom_state_lookback() -> int:
    return 252


@strategy(
    "qc_momentum_and_state_of_market_filters",
    entry="mom_state_entry",
    exit="mom_state_exit",
    prepare_bars=_prepare_momentum_state,
    required_lookback=_mom_state_lookback,
)
def _mom_state() -> None:
    """
    Momentum And State Of Market Filters.
    Approximation: Long-only implementation of the top 20 momentum stocks
    when the market state is UP (SPY 12-month return > 0).
    Short leg and Treasury ETF (TLT) logic omitted due to framework limits.
    """
    pass
