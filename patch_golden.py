import pprint
from screener.backtester.historical import run_backtest
from screener.backtester.rolling import run_rolling_backtest
from tests.test_day_loop import HISTORICAL_SCENARIOS, ROLLING_SCENARIOS, _trade_tuples, _equity_tuples

def generate_golden(scenario_name="dividends"):
    cfg, fetcher, _bars = HISTORICAL_SCENARIOS[scenario_name]()
    res_hist = run_backtest(cfg, fetcher)
    t_hist = _trade_tuples(res_hist)
    e_hist = _equity_tuples(res_hist)
    
    cfg, fetcher, start, end = ROLLING_SCENARIOS[scenario_name]()
    res_roll = run_rolling_backtest(cfg, fetcher, start_date=start, end_date=end)
    t_roll = _trade_tuples(res_roll)
    e_roll = _equity_tuples(res_roll)
    
    print("HISTORICAL_GOLDEN 'dividends':")
    pprint.pprint({"trades": t_hist, "equity": e_hist})
    print("\nROLLING_GOLDEN 'dividends':")
    pprint.pprint({"trades": t_roll, "equity": e_roll})

if __name__ == "__main__":
    generate_golden()
