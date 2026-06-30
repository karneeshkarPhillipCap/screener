"""Mark Minervini Trend Template pipeline criterion."""

from __future__ import annotations

from datetime import date
from typing import Any

from rich.console import Console

from screener.criteria import criterion
from screener.minervini import render_rows, scan_minervini


@criterion("mark-minervini", pipeline=True)
def mark_minervini_pipeline(
    *,
    market: str,
    limit: int,
    refresh: bool,
    cache_ttl: str,
    **_: Any,
) -> None:
    console = Console()
    rows = scan_minervini(
        market,
        as_of=date.today(),
        limit=int(limit),
        cache_ttl=cache_ttl,
        refresh=refresh,
    )
    render_rows(rows, console, market)
    console.print(
        "\n[dim]RS Rank is a universe-relative 12-month percentile proxy, not "
        "Investor's Business Daily's proprietary RS Rating.[/dim]"
    )
