import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_sentiment_rotation(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Benchmark calculations for proxies
    bench = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if not bench.empty:
        bench_close = bench["close"].astype(float)
        bench_ret = bench_close.pct_change()

        # VIX proxy: 20-day rolling std of benchmark
        vix_proxy = bench_ret.rolling(20, min_periods=10).std()

        # PutCallRatio proxy: negative 20-day return (as market falls, puts increase)
        pcr_proxy = -(bench_close / bench_close.shift(20) - 1.0)

        vix_ma1 = vix_proxy.rolling(20).mean()  # ~1 month
        vix_ma6 = vix_proxy.rolling(120).mean()  # ~6 months

        pcr_ma1 = pcr_proxy.rolling(20).mean()
        pcr_ma6 = pcr_proxy.rolling(120).mean()

        vix_up = vix_ma1 > vix_ma6
        pcr_up = pcr_ma1 > pcr_ma6
    else:
        # Defaults if no benchmark
        vix_up = pd.Series(False, index=pd.DatetimeIndex([]))
        pcr_up = pd.Series(False, index=pd.DatetimeIndex([]))

    # Cross-sectional calculation
    closes = {}
    vols = {}
    for sym, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[sym] = bars["close"].astype(float)
            vols[sym] = bars["volume"].astype(float)

    if not closes:
        return ctx.bars_by_tv

    df_close = pd.DataFrame(closes)
    df_vol = pd.DataFrame(vols)

    # 1. Market Cap proxy (Dollar volume)
    dollar_vol = df_close * df_vol
    avg_dollar_vol = dollar_vol.rolling(60).mean()

    # Identify Top 30% by dollar volume
    dv_ranks = avg_dollar_vol.rank(axis=1, pct=True)
    is_large_cap = dv_ranks >= 0.70

    # 2. P/B proxy (1-year return)
    # Value = Low return, Growth = High return
    ret_1y = df_close / df_close.shift(252) - 1.0

    # Rank ret_1y ONLY among large caps
    # We can mask non-large caps with NaN
    large_cap_ret = ret_1y.where(is_large_cap, np.nan)
    ret_ranks = large_cap_ret.rank(axis=1, pct=True)

    # Value = bottom 20% of the large caps (which is roughly bottom 0.20 of the valid ranks)
    is_value = is_large_cap & (ret_ranks <= 0.20)
    is_growth = is_large_cap & (ret_ranks >= 0.80)

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy()

        sym_vix_up = vix_up.reindex(df.index).fillna(False)
        sym_pcr_up = pcr_up.reindex(df.index).fillna(False)

        sym_val = is_value[sym].reindex(df.index).fillna(False)
        sym_gro = is_growth[sym].reindex(df.index).fillna(False)

        # Rotation logic
        # VIX MA1 > VIX MA6 and PCR MA1 < PCR MA6 -> Long Value
        # VIX MA1 > VIX MA6 and PCR MA1 > PCR MA6 -> Short Value (we do Cash/None)
        # VIX MA1 <= VIX MA6 -> Long Value + Growth

        cond_long_value_only = sym_vix_up & (~sym_pcr_up)
        cond_long_both = ~sym_vix_up

        entry = (cond_long_value_only & sym_val) | (
            cond_long_both & (sym_val | sym_gro)
        )

        df["rotation_entry"] = entry
        df["rotation_exit"] = ~entry
        prepared[sym] = df

    return prepared


def _sentiment_rotation_lookback() -> int:
    return 252


@strategy(
    "qc_sentiment_and_style_rotation_effect_in_stocks",
    entry="rotation_entry",
    exit="rotation_exit",
    prepare_bars=_prepare_sentiment_rotation,
    required_lookback=_sentiment_rotation_lookback,
)
def _sentiment_rotation() -> None:
    """
    Sentiment and Style Rotation.
    Approximated Market Cap, P/B, VIX, and Put-Call Ratio with price-volume metrics.
    Short leg dropped to fit long-only framework.
    """
    pass
