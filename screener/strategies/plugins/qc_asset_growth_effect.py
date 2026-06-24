import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_asset_growth(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    growth_dict = {}

    # Calculate proxy asset growth for each symbol
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            continue

        df = bars.copy()
        close = df["close"].astype(float)
        vol = df["volume"].astype(float)

        # Proxy for firm size/assets: Dollar Volume
        dollar_vol = close * vol
        avg_dollar_vol = dollar_vol.rolling(60, min_periods=20).mean()

        # 1-year growth
        growth = (avg_dollar_vol / avg_dollar_vol.shift(252)) - 1.0
        growth_dict[sym] = growth

    if not growth_dict:
        return ctx.bars_by_tv

    growth_df = pd.DataFrame(growth_dict)

    # We want the bottom decile (lowest growth) for the long leg.
    # rank(pct=True) gives percentile. Bottom decile is <= 0.10.
    ranks = growth_df.rank(axis=1, pct=True)
    is_bottom_decile = ranks <= 0.10

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy()
        df["asset_growth_entry"] = is_bottom_decile[sym].reindex(df.index).fillna(False)
        df["asset_growth_exit"] = ~df["asset_growth_entry"]
        prepared[sym] = df

    return prepared


def _asset_growth_lookback() -> int:
    return 312  # 252 for year + 60 for moving avg


@strategy(
    "qc_asset_growth_effect",
    entry="asset_growth_entry",
    exit="asset_growth_exit",
    prepare_bars=_prepare_asset_growth,
    required_lookback=_asset_growth_lookback,
)
def _asset_growth() -> None:
    """
    Asset Growth Effect.
    Approximated using Dollar Volume growth instead of Fundamental Total Assets.
    Longs the bottom decile of growth (long-only).
    """
    pass
