import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_term_structure(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Proxy term structure roll return using the ratio of SMA(20) to SMA(120).
    Sorts assets into quintiles and selects the top 20% (proxy for backwardation)
    and bottom 20% (proxy for contango).
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 120:
            sma_20 = df["close"].rolling(20).mean()
            sma_120 = df["close"].rolling(120).mean()
            df["roll_return_proxy"] = (sma_20 / sma_120) - 1
        else:
            df["roll_return_proxy"] = 0.0

    # Create a panel for the roll return proxy to calculate cross-sectional quantiles
    roll_panel = pd.DataFrame(
        {sym: df["roll_return_proxy"] for sym, df in ctx.bars_by_tv.items()}
    )

    # Calculate quintiles daily across symbols
    # Use quantile to get top 20% and bottom 20%
    q_top = roll_panel.rank(axis=1, pct=True) >= 0.8
    q_bottom = roll_panel.rank(axis=1, pct=True) <= 0.2

    for sym, df in ctx.bars_by_tv.items():
        df["signal"] = 0
        # Signal 1 for long (backwardation), -1 for short (contango)
        df.loc[q_top[sym] & (roll_panel[sym] > 0), "signal"] = 1
        df.loc[q_bottom[sym] & (roll_panel[sym] < 0), "signal"] = -1

    return ctx.bars_by_tv


@strategy(
    "qc_term-structure-effect-in-commodities",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_term_structure,
    required_lookback=lambda: 120,
)
def _qc_term_structure_effect_in_commodities():
    pass
