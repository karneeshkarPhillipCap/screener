import pandas as pd
from datetime import date
from screener.backtester.models import BacktestConfig
from screener.backtester.core import simulate_ticker
from screener.backtester.pine import parse

def main():
    # create some bars
    dates = pd.date_range("2024-01-01", periods=10, freq="B")
    bars = pd.DataFrame({
        "open": [10, 11, 12, 13, 14, 15, 14, 13, 12, 11],
        "high": [11, 12, 13, 14, 15, 16, 15, 14, 13, 12],
        "low": [9, 10, 11, 12, 13, 14, 13, 12, 11, 10],
        "close": [10.5, 11.5, 12.5, 13.5, 14.5, 15.5, 13.5, 12.5, 11.5, 10.5],
        "volume": [100]*10,
    }, index=dates)
    
    cfg = BacktestConfig(
        market="us",
        as_of=date(2024, 1, 15),
        hold=10,
        top=5,
        entry_expr="close > 0",
        exit_expr="close < 14", # at idx=6, close is 13.5
        stop_loss=None,
        take_profit=None,
        trailing_stop=None,
        slippage_bps=0.0,
        commission_bps=0.0,
        initial_capital=100_000.0,
        benchmark="SPY",
    )
    
    # Enter at idx=0 (bar 0) -> trade opens bar 1
    exit_ast = parse(cfg.exit_expr)
    outcome = simulate_ticker(bars, signal_idx=0, cfg=cfg, exit_ast=exit_ast)
    
    trade = outcome.trade
    print(f"Entry Date: {trade.entry_date}, Entry Price: {trade.entry_price}")
    print(f"Exit Date: {trade.exit_date}, Exit Price: {trade.exit_price}")
    print(f"Exit Reason: {trade.exit_reason}")
    print(f"Bars:")
    for i, (idx, row) in enumerate(bars.iterrows()):
        print(f"  {i}: {idx.date()} O={row['open']} C={row['close']}")

if __name__ == '__main__':
    main()
