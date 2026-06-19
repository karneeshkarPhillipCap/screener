import pandas as pd
from screener.backtester.data import _normalize_frame, YFinancePriceFetcher

def test_split_factor():
    df = pd.DataFrame({
        "Open": [100.0, 100.0, 50.0, 50.0],
        "High": [100.0, 100.0, 50.0, 50.0],
        "Low": [100.0, 100.0, 50.0, 50.0],
        "Close": [100.0, 100.0, 50.0, 50.0],
        "Volume": [100, 100, 200, 200],
        "Stock Splits": [0.0, 0.0, 2.0, 0.0]
    }, index=pd.date_range("2024-01-01", periods=4))
    
    out = _normalize_frame(df)
    print("Normalized Frame:")
    print(out[["open", "close", "split_factor", "stock_splits"]])

if __name__ == "__main__":
    test_split_factor()
