"""Stock Selection Strategy Based On Fundamental Factors."""

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_fundamental_factors(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Since we lack Morningstar fundamental data (FCFYield, BookValuePerShare, RevenueGrowth)
    in this environment, we use price-based proxies to form the 4 factors:
    - FCFYield (Quality) -> Inverse 21-day volatility
    - BookValuePerShare (Value) -> Negative distance from 52-week high
    - RevenueGrowth (Growth) -> 252-day return (long-term trend)
    - PriceChange1M (Momentum) -> 21-day return

    We rank each factor cross-sectionally, place into quintiles (1-5), and sum
    them into an equal-weighted composite factor score, picking the top 20 stocks.
    """
    metrics: dict[str, pd.DataFrame] = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
        close = bars["close"].astype(float)

        # Intermediate calculations
        ret_1m = close.pct_change(21)
        ret_12m = close.pct_change(252)
        vol_1m = close.pct_change().rolling(21).std()
        max_52w = close.rolling(252).max()

        # Proxies
        inv_vol = -vol_1m
        value_proxy = -(
            close / max_52w
        )  # more negative = further from high = "cheaper"
        growth_proxy = ret_12m
        mom_proxy = ret_1m

        df_proxy = pd.DataFrame(
            {
                "quality": inv_vol,
                "value": value_proxy,
                "growth": growth_proxy,
                "mom": mom_proxy,
            }
        )
        metrics[sym] = df_proxy

    if not metrics:
        return ctx.bars_by_tv

    # Combine into panels
    panels = {
        factor: pd.DataFrame({sym: df[factor] for sym, df in metrics.items()})
        for factor in ["quality", "value", "growth", "mom"]
    }

    # Resample to monthly to calculate scores
    monthly_scores: dict[str, pd.DataFrame] = {}
    for factor, panel in panels.items():
        panel_monthly = panel.resample("ME").last()
        # Quintile ranking (1 to 5)
        pct_ranks = panel_monthly.rank(axis=1, pct=True)
        # 1 is worst, 5 is best (highest factor value)
        quintiles = pd.DataFrame(
            np.ceil(pct_ranks.to_numpy() * 5),
            index=pct_ranks.index,
            columns=pct_ranks.columns,
        )
        monthly_scores[factor] = quintiles

    # Composite score
    df_composite = sum(monthly_scores.values(), pd.DataFrame()) / 4.0

    # Select top 20 based on composite score
    composite_ranks = df_composite.rank(axis=1, ascending=False)
    top_20_monthly = composite_ranks <= 20

    # Forward fill to daily
    top_20 = top_20_monthly.reindex(panels["mom"].index, method="ffill").fillna(False)

    prepared: dict[str, pd.DataFrame] = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Shift by 1 to avoid lookahead bias (execute day after rebalance)
        df["signal"] = (
            top_20.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        prepared[sym] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_stock_selection_strategy_based_on_fundamental_factors",
    entry="signal > 0",
    exit="signal == 0",
    prepare_bars=_prepare_fundamental_factors,
    required_lookback=_lookback,
)
def _qc_stock_selection_strategy_based_on_fundamental_factors() -> None:
    pass
