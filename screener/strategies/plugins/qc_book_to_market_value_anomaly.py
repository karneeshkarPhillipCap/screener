"""Book-to-Market Value Anomaly strategy implementation."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_bm_anomaly(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximation of the Book-to-Market Value Anomaly.
    Since we lack fundamental data (Book Value, Shares Outstanding) in this environment:
    - Market Cap Proxy: 63-day average of (Close * Volume).
    - B/M Proxy: 252-day SMA of Close / Close.
    We filter top 20% by Market Cap Proxy, then top 20% of those by B/M Proxy.
    Rebalanced annually.
    """
    size_proxies = {}
    bm_proxies = {}

    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue

        close = bars["close"].astype(float)
        volume = bars["volume"].astype(float)

        # Dollar volume proxy for Market Cap
        dollar_vol = close * volume
        avg_dollar_vol = dollar_vol.rolling(63, min_periods=21).mean()
        size_proxies[sym] = avg_dollar_vol

        # B/M Proxy: 252-day SMA / Close
        # A higher ratio implies the stock is cheaper relative to its long-term average price
        sma252 = close.rolling(252, min_periods=63).mean()
        bm_proxy = sma252 / (close + 1e-8)
        bm_proxies[sym] = bm_proxy

    if not size_proxies:
        return ctx.bars_by_tv

    df_size = pd.DataFrame(size_proxies)
    df_bm = pd.DataFrame(bm_proxies)

    # Annual rebalance: resample to end of year
    df_size_yearly = df_size.resample("YE").last()
    df_bm_yearly = df_bm.resample("YE").last()

    # Rank size cross-sectionally
    size_ranks = df_size_yearly.rank(axis=1, pct=True)
    top_20_size = size_ranks >= 0.80

    # Filter BM proxy to only top 20% size
    bm_filtered = df_bm_yearly[top_20_size]

    # Rank BM cross-sectionally among the filtered stocks
    bm_ranks = bm_filtered.rank(axis=1, pct=True)
    # Top 20% of the top 20% -> highest quintile of the size-filtered group
    top_bm_yearly = bm_ranks >= 0.80

    # Forward fill the yearly selection to daily
    top_bm_daily = top_bm_yearly.reindex(df_size.index, method="ffill").fillna(False)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        # Shift the yearly signal by 1 day so we enter on the first day of the new year
        df["bm_signal"] = (
            top_bm_daily.get(sym, pd.Series(False, index=df.index))
            .shift(1)
            .fillna(False)
            .astype(int)
        )
        prepared[sym] = df

    return prepared


def _bm_lookback() -> int:
    # 252 days for SMA
    return 252


@strategy(
    "qc_book-to-market-value-anomaly",
    entry="bm_signal > 0",
    exit="bm_signal == 0",
    prepare_bars=_prepare_bm_anomaly,
    required_lookback=_bm_lookback,
)
def _qc_book_to_market_value_anomaly() -> None:
    """Expression-only strategy."""
