from __future__ import annotations

import pandas as pd
from screener.strategies.registry import get_strategy
from screener.strategies.expressions import resolve_strategy

def test_strategies_registered():
    mean_rev = resolve_strategy("mean_reversion_regime")
    assert mean_rev is not None
    assert mean_rev.entry is not None
    
    dual = resolve_strategy("dual_momentum")
    assert dual is not None
    
    vcp = resolve_strategy("vcp_breakout")
    assert vcp is not None
    
    pead = resolve_strategy("pead_proxy")
    assert pead is not None
