import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_sector_mom(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 12-month (252 trading days) momentum and select top 3."""
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 252:
            df["mom_12m"] = df["close"].pct_change(252)
        else:
            df["mom_12m"] = 0.0

    mom_panel = pd.DataFrame({sym: df["mom_12m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols
    ranks = mom_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in top 3
        df["signal"] = (ranks[sym] <= 3).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_sector-momentum",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_sector_mom,
    required_lookback=lambda: 252,
)
def _qc_sector_momentum():
    pass
