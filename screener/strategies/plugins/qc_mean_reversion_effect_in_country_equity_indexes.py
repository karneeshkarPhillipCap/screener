import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_mean_reversion(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 36-month (756 trading days) return and select bottom 4."""
    lookback = 756

    for sym, df in ctx.bars_by_tv.items():
        if len(df) > lookback:
            df["ret_36m"] = df["close"].pct_change(lookback)
        else:
            df["ret_36m"] = 0.0

    ret_panel = pd.DataFrame({sym: df["ret_36m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols (ascending=True means worst returns get lower ranks)
    ranks = ret_panel.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in bottom 4 (worst returns)
        df["signal"] = (ranks[sym] <= 4).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_mean-reversion-effect-in-country-equity-indexes",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_mean_reversion,
    required_lookback=lambda: 756,
)
def _qc_mean_reversion_effect_in_country_equity_indexes():
    pass
