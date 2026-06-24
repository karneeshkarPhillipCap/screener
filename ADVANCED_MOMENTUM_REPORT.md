# Advanced Momentum Strategies: Synthesis Report

## 1. Strategy Thesis

This research explores and implements three institutional-grade momentum strategies to evaluate whether they can outperform the baseline cross-sectional `dual_momentum` strategy over a rigorous 5-year rolling backtest.

### A. Clenow's Quality Momentum (`clenow_momentum`)
**Logic**: Based on Andreas Clenow's "Stocks on the Move", this strategy measures momentum not just by raw price appreciation, but by the "quality" or smoothness of the trend. 
**Mathematical Rules**: It computes the annualized slope of an exponential regression over a 90-day window, multiplied by the $R^2$ (coefficient of determination) of that regression. This penalizes stocks with erratic jumps and favors consistent, steady climbers. It applies an absolute momentum filter requiring the benchmark (S&P 500 / Nifty 50) to be above its 100-day moving average (tweaked from 200 for faster responsiveness).

### B. Accelerating Dual Momentum (`accelerating_momentum`)
**Logic**: An adaptation of tactical asset allocation ADM applied to equities. Rather than relying on a single lookback window (which is prone to lag or whipsaw), this strategy aggregates momentum across multiple timeframes to capture accelerating trends.
**Mathematical Rules**: Calculates the average of 3-month (63-day), 6-month (126-day), and 12-month (252-day) returns. To protect against deep drawdowns in individual equities, the strategy incorporates a stock-specific absolute momentum filter requiring the stock's closing price to reside above its 200-day simple moving average.

### C. Volatility-Adjusted Momentum (`volatility_momentum`)
**Logic**: High absolute returns are often a byproduct of high underlying volatility. This strategy seeks risk-adjusted momentum by directly penalizing volatile assets.
**Mathematical Rules**: Evaluates the 90-day Rate of Change (ROC) scaled inversely by the 90-day annualized standard deviation of daily returns (resembling a rolling Sharpe ratio). Only stocks with positive adjusted momentum, while the broader market resides in a bull regime (Benchmark > 200 SMA), are considered.

---

## 2. Performance Matrix (5-Year Rolling Backtest)

The backtests were run using a holding period of 20 days, picking the top 10 ranked stocks, with an 8% stop loss.

### US Market Equities
| Strategy | CAGR | Sharpe Ratio | Max Drawdown | Hit Rate | Total Trades |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Dual Momentum (Baseline)** | +18.81% | 1.175 | -18.58% | 46.39% | 720 |
| **Clenow's Quality Momentum** | **+21.05%** | **1.374** | **-15.24%** | **47.50%** | 699 |
| **Volatility-Adjusted Momentum** | +18.88% | 1.185 | -22.83% | 45.11% | 767 |
| **Accelerating Dual Momentum** | +18.99% | 1.170 | -18.64% | 44.92% | 768 |

### India Market Equities
*Note: Due to the structural differences in market breadth, liquidity, and recent broad-based corrections in the tested universe, all momentum configurations, including the baseline, yielded suboptimal risk-adjusted performance.*

| Strategy | CAGR | Sharpe Ratio | Max Drawdown | Hit Rate | Total Trades |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Dual Momentum (Baseline)** | +5.20% | 0.484 | -21.16% | 46.93% | 603 |
| **Accelerating Dual Momentum** | +4.22% | 0.401 | -18.73% | 42.73% | 688 |
| **Clenow's Quality Momentum** | +2.07% | 0.227 | -17.65% | 46.55% | 607 |
| **Volatility-Adjusted Momentum** | +1.32% | 0.168 | -23.91% | 42.19% | 723 |

---

## 3. Deployment Recommendation

Based on the empirical evidence from the 5-year rolling backtest, **Clenow's Quality Momentum is the definitive recommendation for live deployment in US Equities.** 

It fulfills and exceeds the success criteria (Sharpe > 1.17, CAGR > 18%) by delivering a massive **1.374 Sharpe** and **21.05% CAGR**, while substantially reducing maximum drawdown to **-15.24%** (outperforming the baseline's -18.58%). By rewarding high $R^2$ trends, it effectively avoids the volatile "meme stock" spikes that trigger stop-losses, resulting in the highest Hit Rate (47.50%) with fewer total trades.

**Secondary Recommendation:** Volatility-Adjusted Momentum is also viable as a diversifying factor model, edging out the baseline with a 1.185 Sharpe. 

**India Market Caution:** The rigid cross-sectional ranking paradigm combined with an 8% absolute stop-loss proved detrimental in the Indian market universe over the rolling 5-year sample. It is highly recommended to abstain from deploying these strategies live in India until the universe selection criteria or stop-loss mechanisms are overhauled and re-validated for emerging market volatility profiles.
