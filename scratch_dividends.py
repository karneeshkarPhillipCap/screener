import pandas as pd
from datetime import date
from screener.backtester.models import Trade
from screener.backtester.portfolio import Portfolio, build_equity_curve

def test_dividends_in_equity_curve():
    # 1. Setup Portfolio
    port = Portfolio(10000.0, 1)
    
    # 2. Open Position
    pos = port.open("AAPL", date(2024, 1, 1), 100.0, 0.0) # Buys 100 shares
    
    # 3. Credit Dividend
    port.credit_dividends("AAPL", 5.0) # $5 per share * 100 shares = $500 dividend
    print("Actual Portfolio Cash before exit:", port.cash())
    
    # 4. Close Position
    trade = port.close("AAPL", date(2024, 1, 3), 110.0, "eod", 0.0) # Sells 100 shares at 110 = $11000
    print("Actual Portfolio Cash after exit:", port.cash()) # Should be 0 + $500 + $11000 = $11500
    
    print("Trade PnL:", trade.pnl) # $11000 - $10000 = $1000
    print("Trade Dividend Income:", trade.dividend_income) # $500
    
    # 5. Build Equity Curve
    calendar = pd.date_range("2024-01-01", "2024-01-04")
    price_panel = {"AAPL": pd.DataFrame({"close": [100.0, 105.0, 110.0, 110.0]}, index=calendar)}
    
    equity = build_equity_curve(calendar, [trade], price_panel, 10000.0)
    print("\nReconstructed Equity Curve:")
    print(equity)

if __name__ == "__main__":
    test_dividends_in_equity_curve()
