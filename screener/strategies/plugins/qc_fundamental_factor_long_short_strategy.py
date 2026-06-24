"""Fundamental Factor Long Short Strategy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_fundamental_factor(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Since we do not have fundamental data (Book Value, Operating Margin),
    we approximate the three factors using price-based proxies:
    - Value Proxy: Long-term Reversal (Negative of 252-day return)
    - Quality Proxy: Low Volatility (Negative of 252-day return volatility)
    - Momentum: 1-month return (21-day return)

    Scores are computed monthly, and stocks are ranked.
    The strategy goes long the top 10% and short the bottom 10%.
    """
    value_proxy = {}
    quality_proxy = {}
    momentum_factor = {}

    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
        close = bars["close"].astype(float)

        # 1-month return (21 trading days)
        ret_1m = close.pct_change(21)
        # 1-year return (252 trading days)
        ret_1y = close.pct_change(252)
        # 1-year volatility
        vol_1y = close.pct_change().rolling(252).std()

        # Higher is better for our ranking, so we negate where appropriate
        value_proxy[sym] = -ret_1y
        quality_proxy[sym] = -vol_1y
        momentum_factor[sym] = ret_1m

    if not value_proxy:
        return ctx.bars_by_tv

    df_V = pd.DataFrame(value_proxy)
    df_Q = pd.DataFrame(quality_proxy)
    df_M = pd.DataFrame(momentum_factor)

    # Monthly rebalance
    df_V_mo = df_V.resample("ME").last()
    df_Q_mo = df_Q.resample("ME").last()
    df_M_mo = df_M.resample("ME").last()

    # Rank descending (highest value gets rank 1)
    rank_V = df_V_mo.rank(axis=1, ascending=False)
    rank_Q = df_Q_mo.rank(axis=1, ascending=False)
    rank_M = df_M_mo.rank(axis=1, ascending=False)

    # Combined Score (lower is better)
    score = 0.4 * rank_V + 0.4 * rank_Q + 0.2 * rank_M

    # Final percentiles based on score (lowest score is best, so pct_rank near 0 is best)
    pct_rank = score.rank(axis=1, pct=True, ascending=True)

    # Top 10% (best scores) = Long, Bottom 10% (worst scores) = Short
    long_mask_monthly = pct_rank <= 0.10
    short_mask_monthly = pct_rank >= 0.90

    # Reindex back to daily
    long_mask = long_mask_monthly.reindex(df_V.index, method="ffill").fillna(False)
    short_mask = short_mask_monthly.reindex(df_V.index, method="ffill").fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Shift signals by 1 to prevent lookahead bias
        df["long_signal"] = (
            long_mask.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        df["short_signal"] = (
            short_mask.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        # We map signals to position target: 1 for long, -1 for short, 0 for cash
        df["position_target"] = 0
        df.loc[df["long_signal"] == 1, "position_target"] = 1
        df.loc[df["short_signal"] == 1, "position_target"] = -1
        prepared[sym] = df

    return prepared


def _lookback() -> int:
    return 252


@strategy(
    "qc_fundamental_factor_long_short_strategy",
    entry="position_target != 0",
    exit="position_target == 0",
    direction="both",
    prepare_bars=_prepare_fundamental_factor,
    required_lookback=_lookback,
)
def _qc_fundamental_factor_long_short() -> None:
    """Expression-only strategy for fundamental factor long/short proxy."""
