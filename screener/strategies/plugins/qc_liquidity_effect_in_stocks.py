"""Liquidity Effect in Stocks strategy implementation."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_liquidity_effect(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximates the Liquidity Effect using Amihud Illiquidity and Average Dollar Volume.
    """
    adv_dict = {}
    amihud_dict = {}
    price_dict = {}

    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue

        close = bars["close"].astype(float)
        volume = bars["volume"].astype(float)

        dollar_volume = close * volume

        # 252-day ADV
        adv252 = dollar_volume.rolling(252, min_periods=63).mean()

        # Amihud Illiquidity: |Return| / Dollar Volume
        ret = close.pct_change().abs()
        amihud_daily = ret / (dollar_volume + 1e-8)
        amihud252 = amihud_daily.rolling(252, min_periods=63).mean()

        adv_dict[sym] = adv252
        amihud_dict[sym] = amihud252
        price_dict[sym] = close

    if not adv_dict:
        return ctx.bars_by_tv

    df_adv = pd.DataFrame(adv_dict)
    df_amihud = pd.DataFrame(amihud_dict)
    df_price = pd.DataFrame(price_dict)

    # Annual rebalance: resample to end of year
    df_adv_annual = df_adv.resample("YE").last()
    df_amihud_annual = df_amihud.resample("YE").last()
    df_price_annual = df_price.resample("YE").last()

    signal_annual = pd.DataFrame(
        0, index=df_adv_annual.index, columns=df_adv_annual.columns
    )

    for dt in df_adv_annual.index:
        adv_row = df_adv_annual.loc[dt].dropna()
        amihud_row = df_amihud_annual.loc[dt].dropna()
        price_row = df_price_annual.loc[dt].dropna()

        # Intersect valid symbols
        valid_syms = adv_row.index.intersection(amihud_row.index).intersection(
            price_row.index
        )

        # Filter price > 5
        valid_syms = [s for s in valid_syms if price_row[s] > 5]

        if len(valid_syms) < 4:
            continue

        adv_valid = adv_row[valid_syms]
        amihud_valid = amihud_row[valid_syms]

        # Lowest market-cap quartile (lowest 25% of ADV)
        adv_ranks = adv_valid.rank(pct=True)
        small_caps = adv_valid[adv_ranks <= 0.25].index

        if len(small_caps) < 2:
            continue

        # Within small caps, rank by Amihud Illiquidity
        amihud_small = amihud_valid[small_caps]
        amihud_ranks = amihud_small.rank(pct=True)

        # Long highest Amihud (top 5% most illiquid -> lowest turnover proxy)
        longs = amihud_small[amihud_ranks >= 0.95].index
        # Short lowest Amihud (bottom 5% least illiquid -> highest turnover proxy)
        shorts = amihud_small[amihud_ranks <= 0.05].index

        signal_annual.loc[dt, longs] = 1
        signal_annual.loc[dt, shorts] = -1

    # Reindex back to daily, forward fill the annual selection
    signal_daily = signal_annual.reindex(df_adv.index, method="ffill").fillna(0)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
        df = bars.copy().sort_index()
        df["liquidity_signal"] = (
            signal_daily.get(sym, pd.Series(0, index=df.index))
            .shift(1)
            .fillna(0)
            .astype(int)
        )
        prepared[sym] = df

    return prepared


def _liquidity_lookback() -> int:
    # 252 days for average dollar volume and Amihud
    return 252


@strategy(
    "qc_liquidity-effect-in-stocks",
    entry="liquidity_signal == 1",
    exit="liquidity_signal == 0",
    prepare_bars=_prepare_liquidity_effect,
    required_lookback=_liquidity_lookback,
)
def _qc_liquidity() -> None:
    """Expression-only strategy."""
