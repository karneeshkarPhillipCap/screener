"""Financial Modeling Prep provider client."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any, Mapping, Protocol, cast
from urllib.parse import urljoin

import requests

from screener.cache import cached_json_call
from screener.resilience import RetryConfig, call_with_resilience


DEFAULT_STABLE_BASE_URL = "https://financialmodelingprep.com/stable/"
DEFAULT_LEGACY_BASE_URL = "https://financialmodelingprep.com/api/"
_DOTENV_LOADED = False
_FALLBACK_MISSING = object()


class FmpSession(Protocol):
    def get(
        self,
        url: str,
        *,
        params: Mapping[str, object] | None = None,
        timeout: float | tuple[float, float] | None = None,
    ) -> Any: ...


class FmpApiKeyError(ValueError):
    """Raised when an FMP request is attempted without an API key."""


class FmpRequestError(RuntimeError):
    """Raised when FMP returns an unsuccessful HTTP response."""


class FmpPermissionError(FmpRequestError):
    """Raised when FMP rejects the API key or subscription."""


class FmpRateLimitError(FmpRequestError):
    """Raised when FMP rate limits the caller."""


class FmpServerError(FmpRequestError):
    """Raised when FMP returns a 5xx response."""


def load_env_file() -> None:
    """Load simple KEY=VALUE pairs from the project .env if not exported."""
    global _DOTENV_LOADED
    if _DOTENV_LOADED:
        return
    _DOTENV_LOADED = True
    env_path = Path.cwd() / ".env"
    if not env_path.exists():
        return
    for raw_line in env_path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        if not key or key in os.environ:
            continue
        value = value.strip().strip('"').strip("'")
        os.environ[key] = value


def _with_trailing_slash(url: str) -> str:
    return url if url.endswith("/") else f"{url}/"


def _response_text(response: Any) -> str:
    text = getattr(response, "text", "")
    if not text:
        return ""
    return str(text)[:300]


class FmpClient:
    """Small reusable client for Financial Modeling Prep JSON endpoints."""

    def __init__(
        self,
        *,
        api_key: str | None = None,
        base_url: str = DEFAULT_STABLE_BASE_URL,
        legacy_base_url: str = DEFAULT_LEGACY_BASE_URL,
        session: FmpSession | None = None,
        load_env: bool = True,
    ) -> None:
        resolved_key = api_key
        if resolved_key is None:
            resolved_key = self.resolve_api_key(load_env=load_env)
        if not resolved_key:
            raise FmpApiKeyError("FMP_API_KEY is required to use Financial Modeling Prep")
        self.api_key = resolved_key
        self.base_url = _with_trailing_slash(base_url)
        self.legacy_base_url = _with_trailing_slash(legacy_base_url)
        self.session = cast(FmpSession, session or requests.Session())

    @staticmethod
    def resolve_api_key(*, load_env: bool = True) -> str | None:
        if load_env:
            load_env_file()
        return os.environ.get("FMP_API_KEY")

    def url_for(self, endpoint: str, *, legacy: bool = False) -> str:
        if endpoint.startswith(("http://", "https://")):
            return endpoint
        clean = endpoint.strip("/")
        if legacy and clean.startswith("api/"):
            clean = clean.removeprefix("api/")
        if not legacy and clean.startswith("stable/"):
            clean = clean.removeprefix("stable/")
        base = self.legacy_base_url if legacy else self.base_url
        return urljoin(base, clean)

    def get_json(
        self,
        endpoint: str,
        *,
        params: Mapping[str, object] | None = None,
        legacy: bool = False,
        timeout: float | tuple[float, float] | None = 30,
        cache_ttl: float | None = None,
        refresh: bool = False,
        cache_namespace: str = "fmp",
        cache_key: object | None = None,
        fallback: Any = _FALLBACK_MISSING,
        retry: RetryConfig | None = None,
    ) -> Any:
        """GET a JSON endpoint, adding ``apikey`` and optional disk caching."""
        request_params = dict(params or {})
        request_params["apikey"] = self.api_key
        url = self.url_for(endpoint, legacy=legacy)
        operation = endpoint.strip("/") or url
        last_exc: FmpRequestError | None = None

        def request_once() -> Any:
            nonlocal last_exc
            try:
                response = self.session.get(url, params=request_params, timeout=timeout)
                self._raise_for_status(response, operation=operation)
                return response.json()
            except FmpRequestError as exc:
                last_exc = exc
                raise

        def resilient_request() -> Any:
            result = call_with_resilience(
                "fmp",
                operation,
                request_once,
                fallback=fallback,
                retry=retry,
            )
            if result is _FALLBACK_MISSING:
                if last_exc is not None:
                    raise last_exc
                raise FmpRequestError(f"FMP request failed for {operation}")
            return result

        if cache_ttl is not None:
            key = cache_key or ("GET", "legacy" if legacy else "stable", endpoint, params)
            return cached_json_call(
                cache_namespace,
                key,
                ttl_seconds=cache_ttl,
                refresh=refresh,
                fetch=resilient_request,
            )
        return resilient_request()

    def get_legacy_json(self, endpoint: str, **kwargs: Any) -> Any:
        """GET a legacy ``/api/v3`` or ``/api/v4`` FMP endpoint."""
        return self.get_json(endpoint, legacy=True, **kwargs)

    @staticmethod
    def _raise_for_status(response: Any, *, operation: str) -> None:
        status = int(getattr(response, "status_code", 200) or 200)
        if status < 400:
            return
        body = _response_text(response)
        suffix = f": {body}" if body else ""
        if status == 403:
            raise FmpPermissionError(
                f"FMP request forbidden (403) for {operation}; check FMP_API_KEY "
                f"and plan access{suffix}"
            )
        if status == 429:
            headers = getattr(response, "headers", {}) or {}
            retry_after = headers.get("Retry-After") if hasattr(headers, "get") else None
            wait = f"; retry after {retry_after}s" if retry_after else ""
            raise FmpRateLimitError(
                f"FMP rate limit exceeded (429) for {operation}{wait}{suffix}"
            )
        if status >= 500:
            raise FmpServerError(f"FMP server error ({status}) for {operation}{suffix}")
        raise FmpRequestError(f"FMP HTTP error ({status}) for {operation}{suffix}")
