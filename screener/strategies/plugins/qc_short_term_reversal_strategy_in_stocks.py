import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_short_term_reversal(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 1 month (22-day) return and select bottom 10 (long) and top 10 (short)."""
    # Calculate 22-day ROC
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 22:
            df["roc_22"] = df["close"].pct_change(22)
        else:
            df["roc_22"] = 0.0

    # Combine into panel to rank cross-sectionally
    roc_panel = pd.DataFrame({sym: df["roc_22"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols (ascending=False means 1 is the highest return)
    ranks_high = roc_panel.rank(axis=1, ascending=False)
    ranks_low = roc_panel.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # Long bottom 10 (lowest returns)
        long_signal = (ranks_low[sym] <= 10).astype(int)
        # Short top 10 (highest returns)
        short_signal = (ranks_high[sym] <= 10).astype(int) * -1

        df["signal"] = long_signal + short_signal

    return ctx.bars_by_tv


@strategy(
    "qc_short-term-reversal-strategy-in-stocks",
    entry="signal == 1",  # Long
    exit="signal == 0",  # Exit
    prepare_bars=prepare_short_term_reversal,
    required_lookback=lambda: 22,
)
def _qc_short_term_reversal_strategy_in_stocks():
    pass
