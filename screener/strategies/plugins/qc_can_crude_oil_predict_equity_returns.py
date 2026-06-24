import pandas as pd
from scipy.stats import linregress
import yfinance as yf

from screener.strategies.spec import strategy, PrepareCtx


def prepare_oil_equity(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Calculates OLS regression of equity returns on oil returns over a 2-year window.
    Proxy used: USO for crude oil, 2% fixed annual rate for Risk-Free rate.
    """
    if not ctx.bars_by_tv:
        return ctx.bars_by_tv

    sample_df = list(ctx.bars_by_tv.values())[0]
    start = sample_df.index.min()
    end = sample_df.index.max()

    if "USO" in ctx.bars_by_tv:
        oil_df = ctx.bars_by_tv["USO"]
        oil_close = oil_df["close"]
    else:
        # Download USO as proxy for crude oil
        oil_raw = yf.download("USO", start=start, end=end, progress=False)
        if hasattr(oil_raw.columns, "levels"):
            oil_raw.columns = oil_raw.columns.droplevel("Ticker")
        oil_raw.index = (
            oil_raw.index.tz_localize(None)
            if oil_raw.index.tz is not None
            else oil_raw.index
        )
        oil_close = oil_raw["Close"] if "Close" in oil_raw else oil_raw["close"]

    rf_annual = 0.02
    rf_monthly = rf_annual / 12.0

    # Resample oil to monthly
    oil_monthly = oil_close.resample("ME").last()
    oil_returns = oil_monthly.pct_change()

    for sym, df in ctx.bars_by_tv.items():
        if sym == "USO":
            df["signal"] = 0
            continue

        stock_monthly = df["close"].resample("ME").last()
        stock_returns = stock_monthly.pct_change()

        # Align series
        aligned = pd.concat([oil_returns, stock_returns], axis=1).dropna()
        aligned.columns = ["oil", "stock"]

        # Shift oil returns by 1 to predict current month's stock return
        aligned["oil_lag1"] = aligned["oil"].shift(1)
        aligned = aligned.dropna()

        # We will create a daily series for signals
        signal_daily = pd.Series(0, index=df.index)

        for i in range(24, len(aligned)):
            window = aligned.iloc[i - 24 : i]
            x = window["oil_lag1"].values
            y = window["stock"].values

            slope, intercept, r_value, p_value, std_err = linregress(x, y)

            current_oil_lag1 = aligned["oil_lag1"].iloc[i]
            pred = intercept + slope * current_oil_lag1

            # The month we are predicting for is aligned.index[i]
            # The prediction is made at the end of the previous month
            # We can apply this signal for all days in the current month
            current_month_end = aligned.index[i]
            prev_month_end = aligned.index[i - 1]

            mask = (df.index > prev_month_end) & (df.index <= current_month_end)
            signal_daily.loc[mask] = 1 if pred > rf_monthly else 0

        df["signal"] = signal_daily

    return ctx.bars_by_tv


@strategy(
    "qc_can_crude_oil_predict_equity_returns",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_oil_equity,
    required_lookback=lambda: 24 * 21,  # ~24 months of trading days
)
def _qc_can_crude_oil_predict_equity_returns():
    pass
