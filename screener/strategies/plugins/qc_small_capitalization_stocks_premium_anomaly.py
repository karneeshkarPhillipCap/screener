import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_small_cap(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Calculate 30-day average dollar volume as a defensible proxy for market cap,
    since true shares outstanding is not available in standard OHLCV data.
    Select the lowest 10 among those with a price > $5.
    """
    for sym, df in ctx.bars_by_tv.items():
        df["dollar_volume"] = df["close"] * df["volume"]
        df["avg_dollar_volume"] = df["dollar_volume"].rolling(window=30).mean()
        df["price_gt_5"] = df["close"] > 5

    proxy_panel = pd.DataFrame(
        {sym: df["avg_dollar_volume"] for sym, df in ctx.bars_by_tv.items()}
    )

    price_panel = pd.DataFrame(
        {sym: df["price_gt_5"] for sym, df in ctx.bars_by_tv.items()}
    )

    # Mask out those with price <= 5 by setting their proxy value to infinity
    # so they are ranked last.
    proxy_panel_masked = proxy_panel.where(price_panel, float("inf"))

    # Rank daily across symbols (ascending because we want lowest proxy for small caps)
    ranks = proxy_panel_masked.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in bottom 10
        df["signal"] = (ranks[sym] <= 10).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_small-capitalization-stocks-premium-anomaly",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_small_cap,
    required_lookback=lambda: 30,
)
def _qc_small_cap():
    pass
