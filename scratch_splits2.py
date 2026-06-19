import pandas as pd

df = pd.DataFrame({
    "open": [100.0, 100.0, 50.0, 50.0],
    "high": [100.0, 100.0, 50.0, 50.0],
    "low": [100.0, 100.0, 50.0, 50.0],
    "close": [100.0, 100.0, 50.0, 50.0],
    "volume": [100, 100, 200, 200],
    "dividend": [1.0, 0.0, 0.5, 0.0],
    "split_factor": [2.0, 2.0, 1.0, 1.0]
})

factor = df["split_factor"]
for col in ["open", "high", "low", "close", "dividend"]:
    df[col] = df[col] / factor
df["volume"] = df["volume"] * factor

print(df)
