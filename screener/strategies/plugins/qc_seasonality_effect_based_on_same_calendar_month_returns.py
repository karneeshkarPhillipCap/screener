"""Seasonality Effect Based On Same Calendar Month Returns strategy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_seasonality(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    For each stock, calculate the return of the same calendar month from the previous year.
    At the end of month M-1, we predict month M using the return of month M last year.
    Return = Close(M last year) / Close(M-1 last year) - 1
    In monthly terms, this is Close.shift(11) / Close.shift(12) - 1.
    """
    scores = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
        # Get monthly closes
        close = bars["close"].astype(float)
        monthly_close = close.resample("ME").last()

        # Calculate same-month-last-year return
        seasonality_return = monthly_close.shift(11) / monthly_close.shift(12) - 1.0
        scores[sym] = seasonality_return

    if not scores:
        return ctx.bars_by_tv

    df_scores_monthly = pd.DataFrame(scores)

    # Rank cross-sectionally
    ranks_monthly = df_scores_monthly.rank(axis=1, pct=True)

    # Top 10% and Bottom 10%
    long_monthly = ranks_monthly >= 0.90
    short_monthly = ranks_monthly <= 0.10

    # Reindex back to daily and forward fill
    # We want these signals to apply to the daily dataframe
    long_daily = long_monthly.reindex(
        ctx.price_panel.get(ctx.tv_symbols[0], pd.DataFrame()).index, method="ffill"
    ).fillna(False)
    short_daily = short_monthly.reindex(
        ctx.price_panel.get(ctx.tv_symbols[0], pd.DataFrame()).index, method="ffill"
    ).fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Shift 1 day so the signal generated at the end of the month is applied the next day
        df["season_long"] = (
            long_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        df["season_short"] = (
            short_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )

        # We need a single signal column to use both long and short in screener if we want,
        # but the screener only natively supports `entry` and `exit`.
        # For a long/short strategy, we can encode it into a `season_score`
        # 1 for long, -1 for short, 0 for neutral
        df["season_signal"] = df["season_long"] - df["season_short"]
        prepared[sym] = df

    return prepared


def _seasonality_lookback() -> int:
    # We need 13 months of history
    return 13 * 21


@strategy(
    "qc_seasonality-effect-based-on-same-calendar-month-returns",
    entry="season_signal > 0",
    exit="season_signal == 0",
    prepare_bars=_prepare_seasonality,
    required_lookback=_seasonality_lookback,
)
def _qc_seasonality() -> None:
    """Expression-only strategy."""
