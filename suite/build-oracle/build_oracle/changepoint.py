"""
PELT (Pruned Exact Linear Time) changepoint detection.

Detects abrupt changes in the statistical properties (mean) of a
time series using dynamic programming with pruning.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

import numpy as np

ArrayLike = Sequence[float] | np.ndarray


def _to_array(series: ArrayLike) -> np.ndarray:
    a = np.asarray(series, dtype=np.float64)
    if a.ndim != 1:
        raise ValueError("series must be 1-D")
    if len(a) == 0:
        raise ValueError("series must not be empty")
    return a


# ---------------------------------------------------------------------------
# Segment cost (L2 / Gaussian change-in-mean)
# ---------------------------------------------------------------------------


def segment_cost(series: np.ndarray, start: int, end: int) -> float:
    """Cost of the segment ``series[start:end]`` under the L2 model.

    cost = sum((y_i - mean)^2)  for i in [start, end)

    Uses the identity  sum((y_i - mean)^2) = sum(y_i^2) - n * mean^2
    which can be computed in O(1) given cumulative sums.
    """
    if end <= start:
        return 0.0
    seg = series[start:end]
    n = len(seg)
    mean = np.sum(seg) / n
    return float(np.sum((seg - mean) ** 2))


class _CumsumCache:
    """Precomputed cumulative sums for O(1) segment cost queries."""

    def __init__(self, y: np.ndarray):
        self.n = len(y)
        # cumsum[i] = sum(y[0:i])   (cumsum[0] = 0)
        self.cumsum = np.zeros(self.n + 1)
        self.cumsum_sq = np.zeros(self.n + 1)
        for i in range(self.n):
            self.cumsum[i + 1] = self.cumsum[i] + y[i]
            self.cumsum_sq[i + 1] = self.cumsum_sq[i] + y[i] ** 2

    def cost(self, start: int, end: int) -> float:
        """L2 cost for segment [start, end) in O(1)."""
        n_seg = end - start
        if n_seg <= 0:
            return 0.0
        s = self.cumsum[end] - self.cumsum[start]
        sq = self.cumsum_sq[end] - self.cumsum_sq[start]
        return float(sq - (s * s) / n_seg)


# ---------------------------------------------------------------------------
# Penalty functions
# ---------------------------------------------------------------------------


def _penalty_value(penalty: str, n: int) -> float:
    """child safety assessment penalty name to numeric value.

    Common penalties:
        "bic"  -> ln(n)           (Bayesian Information Criterion)
        "aic"  -> 2               (Akaike Information Criterion)
        "mbic" -> 3 * ln(n)       (Modified BIC)
    """
    penalty = penalty.lower()
    if penalty == "bic":
        return math.log(max(n, 2))
    elif penalty == "aic":
        return 2.0
    elif penalty == "mbic":
        return 3.0 * math.log(max(n, 2))
    else:
        raise ValueError(f"Unknown penalty '{penalty}'. Use 'bic', 'aic', or 'mbic'.")


# ---------------------------------------------------------------------------
# PELT algorithm
# ---------------------------------------------------------------------------


def pelt(
    series: ArrayLike,
    penalty: str = "bic",
    min_segment: int = 5,
) -> list[int]:
    """Pruned Exact Linear Time (PELT) changepoint detection.

    Finds the optimal set of changepoint locations that minimise:

        sum_segments( cost(segment) ) + K * n_changepoints

    where K is the penalty and cost is the L2 (change-in-mean) cost.

    Parameters
    ----------
    series : array-like
        1-D time series.
    penalty : str
        ``"bic"`` (default), ``"aic"``, or ``"mbic"``.
    min_segment : int
        Minimum allowed segment length (default 5).

    Returns
    -------
    List of changepoint indices (sorted, 0-based).  Each index marks the
    first observation of a *new* segment.
    """
    y = _to_array(series)
    n = len(y)
    if n < 2 * min_segment:
        return []

    pen = _penalty_value(penalty, n)
    cache = _CumsumCache(y)

    # F[t] = optimal cost for y[0:t]
    F = np.full(n + 1, np.inf)
    F[0] = -pen  # so that F[0] + cost(0, t) + pen = cost(0, t)
    cp_record: dict[int, int] = {}  # cp_record[t] = last changepoint before t

    # Candidate set (pruned)
    candidates = [0]

    for t_star in range(min_segment, n + 1):
        # Find optimal segmentation ending at t_star
        best_f = np.inf
        best_cp = 0

        new_candidates = []
        for tau in candidates:
            seg_start = tau
            seg_end = t_star
            if seg_end - seg_start < min_segment:
                new_candidates.append(tau)
                continue

            cost_val = cache.cost(seg_start, seg_end)
            candidate_f = F[tau] + cost_val + pen

            if candidate_f < best_f:
                best_f = candidate_f
                best_cp = tau

            # PELT pruning: keep tau if F[tau] + cost <= F[t_star]
            # (it might still be useful for future t_star)
            if F[tau] + cost_val <= best_f:
                new_candidates.append(tau)

        F[t_star] = best_f
        cp_record[t_star] = best_cp
        new_candidates.append(t_star)
        candidates = new_candidates

    # Backtrack to recover changepoints
    changepoints = []
    idx = n
    while idx > 0:
        cp = cp_record.get(idx, 0)
        if cp > 0:
            changepoints.append(cp)
        idx = cp

    changepoints.sort()
    return changepoints


# ---------------------------------------------------------------------------
# Confidence scores
# ---------------------------------------------------------------------------


def confidence_scores(
    series: ArrayLike,
    changepoints: list[int],
) -> list[float]:
    """Compute a confidence score for each detected changepoint.

    Uses Cohen's d effect size between adjacent segments, mapped through
    a sigmoid to produce a score in (0, 1).

    Parameters
    ----------
    series : array-like
        The original time series.
    changepoints : list of int
        Changepoint indices as returned by :func:`pelt`.

    Returns
    -------
    List of float confidence scores, one per changepoint.
    """
    y = _to_array(series)
    n = len(y)

    if not changepoints:
        return []

    # Build segment boundaries
    boundaries = [0] + sorted(changepoints) + [n]
    scores = []

    for _i, cp in enumerate(sorted(changepoints)):
        # Find the two segments adjacent to this changepoint
        seg_idx = boundaries.index(cp)
        left_start = boundaries[seg_idx - 1] if seg_idx > 0 else 0
        left_end = cp
        right_start = cp
        right_end = boundaries[seg_idx + 1] if seg_idx + 1 < len(boundaries) else n

        left_seg = y[left_start:left_end]
        right_seg = y[right_start:right_end]

        if len(left_seg) == 0 or len(right_seg) == 0:
            scores.append(0.0)
            continue

        # Cohen's d
        mean_diff = abs(np.mean(right_seg) - np.mean(left_seg))
        n_l, n_r = len(left_seg), len(right_seg)
        var_l = np.var(left_seg, ddof=1) if n_l > 1 else 0.0
        var_r = np.var(right_seg, ddof=1) if n_r > 1 else 0.0
        pooled_std = math.sqrt(((n_l - 1) * var_l + (n_r - 1) * var_r) / max(n_l + n_r - 2, 1))

        if pooled_std == 0:
            cohens_d = float("inf") if mean_diff > 0 else 0.0
        else:
            cohens_d = mean_diff / pooled_std

        # Sigmoid mapping: score = 1 / (1 + exp(-2*(d - 1)))
        # Centers at d=1 (medium effect size)
        try:
            score = 1.0 / (1.0 + math.exp(-2.0 * (cohens_d - 1.0)))
        except OverflowError:
            score = 0.0

        scores.append(float(score))

    return scores
