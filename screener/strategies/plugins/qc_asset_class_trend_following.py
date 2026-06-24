import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_trend_following(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 10-month (210 trading days) SMA and generate signal."""
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 210:
            df["sma_10m"] = df["close"].rolling(210).mean()
        else:
            df["sma_10m"] = float("inf")  # Prevent entry if not enough data

        df["signal"] = (df["close"] > df["sma_10m"]).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_asset-class-trend-following",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_trend_following,
    required_lookback=lambda: 210,
)
def _qc_asset_class_trend_following():
    pass
