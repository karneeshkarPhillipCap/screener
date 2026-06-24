import pandas as pd
import numpy as np
from screener.strategies.spec import PrepareCtx, strategy

def _prepare_qc_optimal_pairs_trading(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()
        
        # Approximate Ornstein-Uhlenbeck mean reversion using Bollinger Bands
        # The original logic uses MLE on pairs spread; we use a single-asset proxy.
        sma_20 = df['close'].rolling(window=20).mean()
        std_20 = df['close'].rolling(window=20).std()
        
        lower_band = sma_20 - 2 * std_20
        
        df['entry_signal'] = df['close'] < lower_band
        df['exit_signal'] = df['close'] > sma_20
        
        prepared[symbol] = df
    return prepared

def _lookback_qc_optimal_pairs_trading() -> int:
    return 20

@strategy(
    "qc_optimal_pairs_trading",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_optimal_pairs_trading,
    required_lookback=_lookback_qc_optimal_pairs_trading,
)
def _qc_optimal_pairs_trading() -> None:
    pass
