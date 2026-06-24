import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_gold_timing(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Prepare bars for Gold Market Timing strategy.

    The original strategy compares S&P 500 Earnings Yield to the 10-Year Bond Yield.
    Since historical S&P 500 Earnings Yield is not natively available via standard OHLCV
    price fetchers, we use a defensible approximation of a constant 5.0% Earnings Yield.

    Entry condition: Earnings Yield > 10-Year Bond Yield * 2.
    """
    # Fetch 10-Year Treasury Yield (^TNX)
    tnx_frames = ctx.fetcher.fetch(["^TNX"], ctx.start, ctx.end)
    tnx_df = tnx_frames.get("^TNX")

    if tnx_df is None or tnx_df.empty:
        # If TNX is unavailable, fallback to an empty series
        tnx_yield = pd.Series(dtype=float)
    else:
        # The 'close' of ^TNX represents the yield in percent (e.g., 4.5 for 4.5%)
        tnx_yield = tnx_df["close"]

    for sym, df in ctx.bars_by_tv.items():
        if df.empty:
            df["signal"] = 0
            continue

        # Align TNX yield with the asset's dataframe
        df["tnx_yield"] = tnx_yield.reindex(df.index).ffill()

        # Approximate S&P 500 Earnings Yield with a constant 5.0%
        # The condition: Earnings Yield > 10-Year Bond Yield * 2
        # If TNX yield is missing, we fill with a high value to avoid false positive signals.
        yields = df["tnx_yield"].fillna(999.0)
        df["signal"] = (5.0 > yields * 2).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_gold-market-timing",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_gold_timing,
    required_lookback=lambda: 0,
)
def _qc_gold_market_timing():
    pass
