import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_short_term_reversal(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 1-month (21 trading days) return and select top 10 / bottom 10."""
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 21:
            df["ret_1m"] = df["close"].pct_change(21)
        else:
            df["ret_1m"] = 0.0

    ret_panel = pd.DataFrame({sym: df["ret_1m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols. 1 is the lowest return.
    ranks_asc = ret_panel.rank(axis=1, ascending=True)
    ranks_desc = ret_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Long the 10 lowest performers (rank_asc <= 10)
        # Short the 10 highest performers (rank_desc <= 10)
        is_long = (ranks_asc[sym] <= 10).astype(int)
        is_short = (ranks_desc[sym] <= 10).astype(int)
        df["signal"] = is_long - is_short

    return ctx.bars_by_tv


@strategy(
    "qc_short-term-reversal",
    entry="signal == 1 or signal == -1",
    exit="signal == 0",
    prepare_bars=prepare_short_term_reversal,
    required_lookback=lambda: 21,
)
def _qc_short_term_reversal():
    pass
