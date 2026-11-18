"""Decorator-driven E2E test registry, metadata validator, and outcome model."""

from __future__ import annotations

import dataclasses
import time
import traceback
from collections.abc import Callable
from typing import Any

ALL_FEATURES: tuple[str, ...] = tuple(f"F{i}" for i in range(1, 16))
VALID_TIERS: tuple[int, ...] = (1, 2, 3, 4)
VALID_NEEDS: frozenset[str] = frozenset({"native", "jvm"})


def normalize_feature_id(raw: str) -> str:
    token = raw.strip().upper()
    if not token.startswith("F"):
        raise ValueError(f"Invalid feature identifier {raw!r}; expected F1..F15.")
    num_part = token[1:]
    if not num_part.isdigit():
        raise ValueError(f"Invalid feature identifier {raw!r}; expected F1..F15.")
    num = int(num_part, 10)
    if not (1 <= num <= 15):
        raise ValueError(f"Feature {raw!r} out of range; expected F1..F15.")
    return f"F{num}"


@dataclasses.dataclass(frozen=True)
class E2eTestSpec:
    test_id: str
    tier: int
    features: tuple[str, ...]
    title: str
    needs: frozenset[str]
    fn: Callable[[Any], None]
    module: str


@dataclasses.dataclass
class E2eTestResult:
    test_id: str
    tier: int
    features: tuple[str, ...]
    title: str
    status: str  # "PASS" | "FAIL" | "ERROR"
    duration_ms: float
    message: str = ""
    traceback_text: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "test_id": self.test_id,
            "tier": self.tier,
            "features": list(self.features),
            "title": self.title,
            "status": self.status,
            "duration_ms": round(self.duration_ms, 2),
            "message": self.message,
            "traceback": self.traceback_text,
        }


_REGISTRY: dict[str, E2eTestSpec] = {}


def e2e_test(
    test_id: str,
    *,
    tier: int,
    features: list[str] | tuple[str, ...],
    title: str,
    needs: list[str] | tuple[str, ...] | set[str] = (),
) -> Callable[[Callable[[Any], None]], Callable[[Any], None]]:
    """Register a genuine opaque-box E2E test function."""
    clean_id = test_id.strip()
    if not clean_id:
        raise ValueError("test_id must not be empty.")
    if tier not in VALID_TIERS:
        raise ValueError(f"Test {clean_id}: invalid tier {tier!r}; expected 1..4.")
    norm_features = tuple(dict.fromkeys(normalize_feature_id(f) for f in features))
    if not norm_features:
        raise ValueError(f"Test {clean_id}: must declare at least one feature in F1..F15.")
    clean_title = title.strip()
    if not clean_title:
        raise ValueError(f"Test {clean_id}: title must not be empty.")
    need_set = frozenset(needs)
    unknown_needs = need_set - VALID_NEEDS
    if unknown_needs:
        raise ValueError(f"Test {clean_id}: unknown needs {sorted(unknown_needs)}.")
    if "jvm" in need_set:
        need_set = need_set | {"native"}

    def decorator(fn: Callable[[Any], None]) -> Callable[[Any], None]:
        if clean_id in _REGISTRY:
            existing = _REGISTRY[clean_id]
            raise ValueError(
                f"Duplicate E2E test_id {clean_id!r} in {fn.__module__} "
                f"(already registered in {existing.module})."
            )
        _REGISTRY[clean_id] = E2eTestSpec(
            test_id=clean_id,
            tier=tier,
            features=norm_features,
            title=clean_title,
            needs=need_set,
            fn=fn,
            module=fn.__module__,
        )
        return fn

    return decorator


def get_all_tests() -> list[E2eTestSpec]:
    return sorted(_REGISTRY.values(), key=lambda t: (t.tier, t.test_id))


def filter_tests(
    *,
    tiers: list[int] | None = None,
    features: list[str] | None = None,
    test_filter: str | None = None,
) -> list[E2eTestSpec]:
    specs = get_all_tests()
    if tiers:
        tier_set = set(tiers)
        specs = [s for s in specs if s.tier in tier_set]
    if features:
        feat_set = {normalize_feature_id(f) for f in features}
        specs = [s for s in specs if any(f in feat_set for f in s.features)]
    if test_filter:
        needle = test_filter.strip().lower()
        exact = [s for s in specs if s.test_id.lower() == needle]
        if exact:
            specs = exact
        else:
            specs = [
                s
                for s in specs
                if needle in s.test_id.lower() or needle in s.title.lower()
            ]
    return specs


def summarize_registry(specs: list[E2eTestSpec] | None = None) -> dict[str, Any]:
    items = specs if specs is not None else get_all_tests()
    by_tier = {t: 0 for t in VALID_TIERS}
    by_tier_feature: dict[int, dict[str, int]] = {
        t: {f: 0 for f in ALL_FEATURES} for t in VALID_TIERS
    }
    by_feature_total = {f: 0 for f in ALL_FEATURES}
    for spec in items:
        by_tier[spec.tier] += 1
        for feat in spec.features:
            by_tier_feature[spec.tier][feat] += 1
            by_feature_total[feat] += 1
    return {
        "total": len(items),
        "by_tier": by_tier,
        "by_tier_feature": by_tier_feature,
        "by_feature_total": by_feature_total,
    }


def validate_quantitative_thresholds() -> dict[str, Any]:
    specs = get_all_tests()
    summary = summarize_registry(specs)
    errors: list[str] = []

    if summary["by_tier"][1] < 75:
        errors.append(f"Tier 1 has {summary['by_tier'][1]} tests (minimum required: 75).")
    if summary["by_tier"][2] < 75:
        errors.append(f"Tier 2 has {summary['by_tier'][2]} tests (minimum required: 75).")
    if summary["by_tier"][3] < 15:
        errors.append(f"Tier 3 has {summary['by_tier'][3]} tests (minimum required: 15).")
    if summary["by_tier"][4] < 8:
        errors.append(f"Tier 4 has {summary['by_tier'][4]} tests (minimum required: 8).")
    if summary["total"] < 173:
        errors.append(f"Total suite has {summary['total']} tests (minimum required: 173).")

    for tier in (1, 2):
        for feat in ALL_FEATURES:
            count = summary["by_tier_feature"][tier][feat]
            if count < 5:
                errors.append(
                    f"Tier {tier} feature {feat} has {count} tests (minimum required: 5)."
                )

    if errors:
        raise AssertionError("E2E quantitative threshold validation failed:\n  - " + "\n  - ".join(errors))
    return summary


def execute_test_spec(spec: E2eTestSpec, ctx: Any) -> E2eTestResult:
    t0 = time.monotonic()
    try:
        spec.fn(ctx)
        elapsed_ms = (time.monotonic() - t0) * 1000.0
        return E2eTestResult(
            test_id=spec.test_id,
            tier=spec.tier,
            features=spec.features,
            title=spec.title,
            status="PASS",
            duration_ms=elapsed_ms,
        )
    except AssertionError as exc:
        elapsed_ms = (time.monotonic() - t0) * 1000.0
        msg = str(exc).strip() or "AssertionError (no message)"
        return E2eTestResult(
            test_id=spec.test_id,
            tier=spec.tier,
            features=spec.features,
            title=spec.title,
            status="FAIL",
            duration_ms=elapsed_ms,
            message=msg,
        )
    except Exception as exc:  # noqa: BLE001
        elapsed_ms = (time.monotonic() - t0) * 1000.0
        tb = traceback.format_exc()
        return E2eTestResult(
            test_id=spec.test_id,
            tier=spec.tier,
            features=spec.features,
            title=spec.title,
            status="ERROR",
            duration_ms=elapsed_ms,
            message=f"{type(exc).__name__}: {exc}",
            traceback_text=tb,
        )
