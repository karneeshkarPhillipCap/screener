import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_vol_premium(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Defensible approximation:
    Original strategy sells ATM straddles and buys OTM puts.
    Without options data, we capture the low-volatility premium
    by selecting the 10 stocks with the lowest 21-day historical volatility.
    """
    # Calculate 21-day return volatility
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 21:
            ret = df["close"].pct_change()
            df["vol_21d"] = ret.rolling(21).std()
        else:
            df["vol_21d"] = float("inf")

    vol_panel = pd.DataFrame({sym: df["vol_21d"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols (ascending = lowest vol gets rank 1)
    ranks = vol_panel.rank(axis=1, ascending=True)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if in bottom 10 of volatility
        # We also require vol_21d to be valid (not inf or NaN)
        df["signal"] = (
            (ranks[sym] <= 10)
            & (df["vol_21d"].notna())
            & (df["vol_21d"] != float("inf"))
        ).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_volatility-risk-premium-effect",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_vol_premium,
    required_lookback=lambda: 22,
)
def _qc_volatility_risk_premium_effect():
    pass
