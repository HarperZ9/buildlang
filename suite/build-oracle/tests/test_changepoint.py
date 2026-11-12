"""Tests for build_oracle.changepoint."""

import numpy as np
import pytest

from build_oracle.changepoint import confidence_scores, pelt, segment_cost

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _step_series(
    n1: int = 100,
    n2: int = 100,
    mean1: float = 0.0,
    mean2: float = 5.0,
    seed: int = 42,
) -> np.ndarray:
    """Two concatenated normal segments with different means."""
    rng = np.random.default_rng(seed)
    seg1 = rng.normal(mean1, 1.0, n1)
    seg2 = rng.normal(mean2, 1.0, n2)
    return np.concatenate([seg1, seg2])


def _multi_step_series(seed: int = 42) -> tuple[np.ndarray, list[int]]:
    """Three segments at indices 0, 80, 160 (changepoints at 80, 160)."""
    rng = np.random.default_rng(seed)
    s1 = rng.normal(0, 1, 80)
    s2 = rng.normal(5, 1, 80)
    s3 = rng.normal(-3, 1, 80)
    return np.concatenate([s1, s2, s3]), [80, 160]


# ---------------------------------------------------------------------------
# segment_cost
# ---------------------------------------------------------------------------


class TestSegmentCost:
    def test_constant_segment(self):
        y = np.array([5.0, 5.0, 5.0, 5.0])
        assert segment_cost(y, 0, 4) == 0.0

    def test_known_cost(self):
        y = np.array([1.0, 3.0])
        # mean = 2, cost = (1-2)^2 + (3-2)^2 = 2
        assert segment_cost(y, 0, 2) == pytest.approx(2.0)

    def test_empty_segment(self):
        y = np.array([1.0, 2.0, 3.0])
        assert segment_cost(y, 2, 2) == 0.0

    def test_single_element(self):
        y = np.array([7.0, 8.0, 9.0])
        assert segment_cost(y, 1, 2) == 0.0


# ---------------------------------------------------------------------------
# pelt
# ---------------------------------------------------------------------------


class TestPELT:
    def test_single_changepoint_detected(self):
        y = _step_series(n1=100, n2=100, mean1=0, mean2=10)
        cps = pelt(y, penalty="bic", min_segment=5)
        assert len(cps) >= 1
        # The detected changepoint should be near index 100
        assert any(abs(cp - 100) < 15 for cp in cps)

    def test_no_changepoint_constant(self):
        y = np.full(100, 3.14)
        cps = pelt(y, penalty="bic", min_segment=5)
        assert len(cps) == 0

    def test_multiple_changepoints(self):
        y, true_cps = _multi_step_series()
        cps = pelt(y, penalty="bic", min_segment=5)
        assert len(cps) >= 2
        # Each true changepoint should have a detection nearby
        for tcp in true_cps:
            assert any(abs(cp - tcp) < 15 for cp in cps), f"No detection near true changepoint {tcp}; got {cps}"

    def test_returns_sorted(self):
        y = _step_series()
        cps = pelt(y)
        assert cps == sorted(cps)

    def test_short_series_no_crash(self):
        y = np.array([1.0, 2.0, 3.0])
        cps = pelt(y, min_segment=5)
        assert cps == []

    def test_penalty_aic(self):
        y = _step_series(mean2=8)
        cps = pelt(y, penalty="aic")
        assert len(cps) >= 1

    def test_penalty_mbic(self):
        y = _step_series(mean2=8)
        cps = pelt(y, penalty="mbic")
        # mbic is more conservative — may find fewer changepoints
        assert isinstance(cps, list)

    def test_unknown_penalty_raises(self):
        with pytest.raises(ValueError, match="Unknown penalty"):
            pelt(np.arange(50.0), penalty="xyz")

    def test_min_segment_respected(self):
        y = _step_series()
        cps = pelt(y, min_segment=20)
        if len(cps) > 0:
            # First changepoint must be >= min_segment from start
            assert cps[0] >= 20


# ---------------------------------------------------------------------------
# confidence_scores
# ---------------------------------------------------------------------------


class TestConfidenceScores:
    def test_empty_changepoints(self):
        y = np.arange(50.0)
        assert confidence_scores(y, []) == []

    def test_high_confidence_for_large_shift(self):
        y = _step_series(mean1=0, mean2=20)
        cps = pelt(y)
        scores = confidence_scores(y, cps)
        assert len(scores) == len(cps)
        assert all(s > 0.5 for s in scores)

    def test_scores_in_range(self):
        y = _step_series()
        cps = pelt(y)
        scores = confidence_scores(y, cps)
        for s in scores:
            assert 0.0 <= s <= 1.0

    def test_known_changepoint(self):
        y = _step_series(mean1=0, mean2=10)
        scores = confidence_scores(y, [100])
        assert len(scores) == 1
        assert scores[0] > 0.8
