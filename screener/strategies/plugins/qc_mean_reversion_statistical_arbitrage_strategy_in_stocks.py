"""Mean Reversion Statistical Arbitrage Strategy In Stocks."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_stat_arb(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    PCA-based statistical arbitrage proxy.
    Instead of a full rolling PCA which is computationally heavy, we use a 1-factor model
    where the factor is the cross-sectional mean of log prices (the 'Market' factor).
    We calculate the rolling beta and alpha of each stock's log price to the market log price,
    extract the residual, and compute its z-score.
    We long stocks with z-score < -1.5 and short stocks with z-score > 1.5.
    """
    closes = {
        sym: bars["close"].astype(float)
        for sym, bars in ctx.bars_by_tv.items()
        if bars is not None and not bars.empty
    }
    if not closes:
        return ctx.bars_by_tv

    df_closes = pd.DataFrame(closes)
    log_p = pd.DataFrame(
        np.log(df_closes.to_numpy()),
        index=df_closes.index,
        columns=df_closes.columns,
    ).ffill()

    # The 1st principal component proxy is the equal-weighted mean of log prices
    market_log_p = log_p.mean(axis=1)

    market_var = market_log_p.rolling(252, min_periods=63).var()
    market_mean = market_log_p.rolling(252, min_periods=63).mean()

    residuals = {}
    for sym in log_p.columns:
        cov = log_p[sym].rolling(252, min_periods=63).cov(market_log_p)
        b = cov / market_var
        a = log_p[sym].rolling(252, min_periods=63).mean() - b * market_mean
        res = log_p[sym] - (a + b * market_log_p)
        residuals[sym] = res

    df_res = pd.DataFrame(residuals)
    zscore = (df_res - df_res.rolling(252, min_periods=63).mean()) / (
        df_res.rolling(252, min_periods=63).std() + 1e-8
    )

    # Rebalance every month (or 30 days). We'll use end-of-month resampling.
    zscore_monthly = zscore.resample("ME").last()

    long_monthly = zscore_monthly < -1.5
    short_monthly = zscore_monthly > 1.5

    long_daily = long_monthly.reindex(df_res.index, method="ffill").fillna(False)
    short_daily = short_monthly.reindex(df_res.index, method="ffill").fillna(False)

    prepared: dict[str, pd.DataFrame] = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        df["stat_arb_long"] = (
            long_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        df["stat_arb_short"] = (
            short_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )

        df["stat_arb_signal"] = df["stat_arb_long"] - df["stat_arb_short"]
        prepared[sym] = df

    return prepared


def _stat_arb_lookback() -> int:
    return 252 * 2


@strategy(
    "qc_mean-reversion-statistical-arbitrage-strategy-in-stocks",
    entry="stat_arb_signal > 0",
    exit="stat_arb_signal == 0",
    prepare_bars=_prepare_stat_arb,
    required_lookback=_stat_arb_lookback,
)
def _qc_stat_arb() -> None:
    """Expression-only strategy."""
