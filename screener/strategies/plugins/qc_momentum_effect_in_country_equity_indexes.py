import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 6-month (126 trading days) momentum and rank to select top 5."""
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 126:
            df["mom_6m"] = df["close"].pct_change(126)
        else:
            df["mom_6m"] = 0.0

    mom_panel = pd.DataFrame({sym: df["mom_6m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols
    ranks = mom_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in top 5
        df["signal"] = (ranks[sym] <= 5).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_momentum-effect-in-country-equity-indexes",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_momentum,
    required_lookback=lambda: 126,
)
def _qc_momentum_effect_in_country_equity_indexes():
    pass
