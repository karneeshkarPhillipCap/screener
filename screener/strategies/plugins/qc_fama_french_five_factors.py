"""Fama French Five Factors strategy proxy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_ff5(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Because the Fama-French Five Factor model requires fundamental data
    (Book Value, Total Equity, Operating Margin, ROE, Asset Growth),
    we create a purely price-based proxy for these factors:
    1. Value (HML) Proxy: 3-year reversal (stocks that have declined over 3 years are "cheap").
    2. Size (SMB) Proxy: High daily volatility (small caps tend to be more volatile).
    3. Quality/Profitability (RMW) Proxy: High 1-year Sharpe ratio (consistent returns).
    4. Investment (CMA) Proxy: Low 1-year Beta (conservative asset growth often correlates with lower market beta).
    We rank stocks on these 4 proxies and combine them to form a final score.
    """
    # 1. Calculate market proxy for beta
    closes = {
        sym: bars["close"].astype(float)
        for sym, bars in ctx.bars_by_tv.items()
        if bars is not None and not bars.empty
    }
    if not closes:
        return ctx.bars_by_tv

    df_closes = pd.DataFrame(closes).ffill()
    daily_ret = df_closes.pct_change()
    market_ret = daily_ret.mean(axis=1)
    market_var = market_ret.rolling(252, min_periods=63).var()

    value_proxy = -df_closes.pct_change(756)  # 3 year reversal
    size_proxy = daily_ret.rolling(252, min_periods=63).std()  # High vol

    # Sharpe = mean / std
    mean_ret = daily_ret.rolling(252, min_periods=63).mean()
    quality_proxy = mean_ret / (size_proxy + 1e-8)

    # Beta = cov / var
    covs = daily_ret.rolling(252, min_periods=63).cov(market_ret)
    beta = covs.divide(market_var, axis=0)
    invest_proxy = -beta  # Low beta

    # Resample monthly to rank
    v_m = value_proxy.resample("ME").last().rank(axis=1, pct=True)
    s_m = size_proxy.resample("ME").last().rank(axis=1, pct=True)
    q_m = quality_proxy.resample("ME").last().rank(axis=1, pct=True)
    i_m = invest_proxy.resample("ME").last().rank(axis=1, pct=True)

    # Combined score (equal weight of ranks)
    combined = (v_m + s_m + q_m + i_m) / 4.0

    # Top 5 and Bottom 5 cross sectionally?
    # Since we don't know the universe size, we'll use top 5% and bottom 5%
    long_monthly = combined >= 0.95
    short_monthly = combined <= 0.05

    long_daily = long_monthly.reindex(df_closes.index, method="ffill").fillna(False)
    short_daily = short_monthly.reindex(df_closes.index, method="ffill").fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        df["ff5_long"] = (
            long_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        df["ff5_short"] = (
            short_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )

        df["ff5_signal"] = df["ff5_long"] - df["ff5_short"]
        prepared[sym] = df

    return prepared


def _ff5_lookback() -> int:
    return 756


@strategy(
    "qc_fama-french-five-factors",
    entry="ff5_signal > 0",
    exit="ff5_signal == 0",
    prepare_bars=_prepare_ff5,
    required_lookback=_ff5_lookback,
)
def _qc_ff5() -> None:
    """Expression-only strategy."""
