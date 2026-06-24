import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_overnight_anomaly(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Defensible approximation of the Overnight Anomaly.

    Since the strategy buys at the close and sells at the next open every single day,
    it cannot be easily represented as a multi-day hold in a daily-bar backtester.
    Instead, we compute the exact overnight return (Open / previous Close - 1)
    minus the daily round-trip transaction costs, and build a synthetic price curve.

    The backtester will then buy and hold this synthetic asset, capturing the exact
    compounded returns of the daily overnight trading strategy.
    """
    # Daily round trip requires 2 trades (buy and sell)
    daily_cost_bps = (ctx.cfg.commission_bps + ctx.cfg.slippage_bps) * 2
    daily_cost_frac = daily_cost_bps / 10000.0

    out = {}
    for sym, df in ctx.bars_by_tv.items():
        if df.empty:
            out[sym] = df
            continue

        # Copy to avoid mutating the benchmark or shared panel data
        df = df.copy()

        # Overnight return: Open(T) / Close(T-1) - 1
        overnight_ret = (df["open"] / df["close"].shift(1)) - 1.0

        # Net return after daily costs
        net_ret = overnight_ret - daily_cost_frac

        # Synthetic price starting at 100
        synth_price = 100.0 * (1.0 + net_ret.fillna(0)).cumprod()

        # Replace OHLC with the synthetic price so the backtester trades it
        df["open"] = synth_price
        df["high"] = synth_price
        df["low"] = synth_price
        df["close"] = synth_price

        # Signal is always 1 so we enter immediately and hold the synthetic asset
        df["signal"] = 1
        out[sym] = df

    return out


@strategy(
    "qc_overnight-anomaly",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_overnight_anomaly,
    required_lookback=lambda: 1,
)
def _qc_overnight_anomaly():
    pass
