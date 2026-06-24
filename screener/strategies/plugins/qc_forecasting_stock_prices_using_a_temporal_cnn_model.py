import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_forecasting_stock_prices_using_a_temporal_cnn_model(
    ctx: PrepareCtx,
) -> dict[str, pd.DataFrame]:
    prepared = {}

    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()

        # Approximate CNN pattern detection with short-term SMA crossovers
        sma5 = df["close"].rolling(window=5).mean()
        sma15 = df["close"].rolling(window=15).mean()

        # Entry when 5-day SMA crosses above 15-day SMA
        df["entry_signal"] = (sma5 > sma15) & (sma5.shift(1) <= sma15.shift(1))

        # Exit when 5-day SMA crosses below 15-day SMA
        df["exit_signal"] = (sma5 < sma15) & (sma5.shift(1) >= sma15.shift(1))

        prepared[symbol] = df

    return prepared


def _lookback_qc_forecasting_stock_prices_using_a_temporal_cnn_model() -> int:
    return 15


@strategy(
    "qc_forecasting_stock_prices_using_a_temporal_cnn_model",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_forecasting_stock_prices_using_a_temporal_cnn_model,
    required_lookback=_lookback_qc_forecasting_stock_prices_using_a_temporal_cnn_model,
)
def _qc_forecasting_stock_prices_using_a_temporal_cnn_model() -> None:
    pass
