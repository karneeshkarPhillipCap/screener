import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_low_vol(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate 252-day volatility of returns and select lowest 5."""
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 2:
            rets = df["close"].pct_change()
            df["vol_252d"] = rets.rolling(252).std()
        else:
            df["vol_252d"] = float("inf")

    vol_panel = pd.DataFrame(
        {sym: df["vol_252d"] for sym, df in ctx.bars_by_tv.items()}
    )

    # Rank daily across symbols. Lowest volatility gets lowest rank (1, 2, 3...)
    ranks = vol_panel.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in top 5 (lowest volatility)
        df["signal"] = (ranks[sym] <= 5).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_volatility-effect-in-stocks",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_low_vol,
    required_lookback=lambda: 252,
)
def _qc_volatility_effect_in_stocks():
    pass
