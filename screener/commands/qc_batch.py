"""Click command for QuantConnect batch backtesting."""

from __future__ import annotations

import click
import pandas as pd
from rich.console import Console
from rich.table import Table

from screener.backtester.models import BacktestConfig
from screener.backtester.rolling import run_rolling_backtest
from screener.backtester.data import YFinancePriceFetcher
from screener.strategies.spec import registry

import datetime
from dateutil.relativedelta import relativedelta

@click.command(name="qc-batch")
@click.option("--csv", "output_csv", is_flag=True, help="Output as CSV.")
@click.option("--years", type=int, default=5, help="Number of years to backtest.")
def qc_batch(output_csv: bool, years: int) -> None:
    """Run batch backtest of all implemented QuantConnect strategies."""
    console = Console()
    
    from screener.strategies.registry import discover_plugins
    from screener.strategies.spec import registry
    discover_plugins()
    strategies = [name for name, _ in registry.items() if name.startswith("qc_")]
    
    if not strategies:
        click.echo("No QuantConnect strategies implemented yet.", err=output_csv)
        return
        
    results_list = []
    
    with open("us_universe.txt") as f:
        us_tickers = tuple(line.strip() for line in f if line.strip()) + ("SPY",)
        
    with open("india_universe.txt") as f:
        india_tickers = tuple(line.strip() for line in f if line.strip()) + ("^NSEI",)
        
    MARKETS_CFG = {
        "us": {"tickers": us_tickers, "benchmark": "SPY"},
        "india": {"tickers": india_tickers, "benchmark": "^NSEI"},
    }
    
    # We will need a progress indicator or just print out as we go
    with console.status("[bold green]Running QuantConnect Batch Backtest...") as status:
        for name in strategies:
            for market in ["us", "india"]:
                status.update(f"[bold green]Running {name} on {market.upper()}...")
                
                end_dt = datetime.date.today()
                start_dt = end_dt - relativedelta(years=years)
                
                spec = registry.get(name)
                entry_expr = spec.entry if spec and spec.entry else "1"
                exit_expr = spec.exit if spec and spec.exit else "0"
                
                cfg = BacktestConfig(
                    market=market,
                    as_of=end_dt,
                    hold=252,
                    top=10,
                    entry_expr=entry_expr,
                    exit_expr=exit_expr,
                    stop_loss=None,
                    take_profit=None,
                    trailing_stop=None,
                    slippage_bps=0.0,
                    commission_bps=0.0,
                    benchmark=MARKETS_CFG[market]["benchmark"],
                    strategy_name=name,
                    tickers=MARKETS_CFG[market]["tickers"],
                    initial_capital=100000.0,
                )
                
                try:
                    res = run_rolling_backtest(cfg, YFinancePriceFetcher(), start_date=start_dt, end_date=end_dt)
                    metrics = res.metrics
                    
                    results_list.append({
                        "Strategy": name,
                        "Market": market.upper(),
                        "CAGR": metrics.get("cagr", 0.0),
                        "Sharpe": metrics.get("sharpe", 0.0),
                        "Max Drawdown": metrics.get("max_drawdown", 0.0),
                        "Volatility": metrics.get("volatility", 0.0),
                        "Start Date": res.config.start_date if hasattr(res.config, 'start_date') else start_dt,
                        "End Date": res.config.as_of,
                        "Status": "Success"
                    })
                except Exception as e:
                    results_list.append({
                        "Strategy": name,
                        "Market": market.upper(),
                        "CAGR": 0.0,
                        "Sharpe": 0.0,
                        "Max Drawdown": 0.0,
                        "Volatility": 0.0,
                        "Start Date": None,
                        "End Date": None,
                        "Status": f"Error: {e}"
                    })
                
    df = pd.DataFrame(results_list)
    if output_csv:
        print(df.to_csv(index=False))
        return
        
    # Print aggregate comparison
    table = Table(title="QuantConnect Strategies - Comparison Report")
    table.add_column("Strategy")
    table.add_column("Market")
    table.add_column("CAGR", justify="right")
    table.add_column("Sharpe", justify="right")
    table.add_column("Max Drawdown", justify="right")
    table.add_column("Volatility", justify="right")
    table.add_column("Status")
    
    # Sort by Sharpe
    df = df.sort_values(by="Sharpe", ascending=False)
    
    for _, row in df.iterrows():
        table.add_row(
            row["Strategy"],
            row["Market"],
            f"{row['CAGR']:.2%}" if pd.notnull(row['CAGR']) else "-",
            f"{row['Sharpe']:.2f}" if pd.notnull(row['Sharpe']) else "-",
            f"{row['Max Drawdown']:.2%}" if pd.notnull(row['Max Drawdown']) else "-",
            f"{row['Volatility']:.2%}" if pd.notnull(row['Volatility']) else "-",
            str(row["Status"])[:30]
        )
        
    console.print(table)
