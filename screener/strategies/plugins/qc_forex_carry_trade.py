import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_carry_proxy(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Proxy for carry trade: lacking interest rate data, we use 3-month momentum
    as a placeholder for cross-sectional ranking.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 63:
            df["mom_3m"] = df["close"].pct_change(63)
        else:
            df["mom_3m"] = 0.0

    mom_panel = pd.DataFrame({sym: df["mom_3m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols. Highest rank (1) = highest momentum
    ranks = mom_panel.rank(axis=1, ascending=False)
    # Lowest rank (max) = lowest momentum
    ranks_asc = mom_panel.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # signal = 1 for the highest, -1 for the lowest, 0 otherwise
        is_highest = (ranks[sym] == 1).astype(int)
        is_lowest = (ranks_asc[sym] == 1).astype(int)
        df["signal"] = is_highest - is_lowest

    return ctx.bars_by_tv


@strategy(
    "qc_forex-carry-trade",
    entry="signal == 1 or signal == -1",
    exit="signal == 0",
    prepare_bars=prepare_carry_proxy,
    required_lookback=lambda: 63,
)
def _qc_forex_carry_trade():
    pass
