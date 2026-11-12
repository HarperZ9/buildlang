"""Tests for build_oracle.metrics."""

import math

import numpy as np
import pytest

from build_oracle.metrics import mae, mape, mase, mse, r_squared, rmse, smape

# ---------------------------------------------------------------------------
# MAE
# ---------------------------------------------------------------------------


class TestMAE:
    def test_perfect_prediction(self):
        assert mae([1, 2, 3], [1, 2, 3]) == 0.0

    def test_known_value(self):
        assert mae([1, 2, 3], [2, 3, 4]) == 1.0

    def test_negative_errors_cancel(self):
        # Errors are absolute, so direction doesn't matter
        assert mae([0, 0], [1, -1]) == 1.0

    def test_single_element(self):
        assert mae([5], [3]) == 2.0


# ---------------------------------------------------------------------------
# MSE / RMSE
# ---------------------------------------------------------------------------


class TestMSE:
    def test_perfect(self):
        assert mse([1, 2, 3], [1, 2, 3]) == 0.0

    def test_known_value(self):
        # errors: 1, 1, 1 -> squared: 1, 1, 1 -> mean = 1
        assert mse([1, 2, 3], [2, 3, 4]) == 1.0

    def test_rmse_is_sqrt_mse(self):
        a, p = [0, 0, 0], [3, 4, 0]
        assert rmse(a, p) == pytest.approx(math.sqrt(mse(a, p)))

    def test_rmse_known(self):
        # errors: 1, -1 -> squared: 1, 1 -> mean = 1 -> sqrt = 1
        assert rmse([1, 2], [2, 1]) == 1.0


# ---------------------------------------------------------------------------
# MAPE / sMAPE
# ---------------------------------------------------------------------------


class TestMAPE:
    def test_perfect(self):
        assert mape([1, 2, 3], [1, 2, 3]) == 0.0

    def test_known_value(self):
        # |1-2|/1=1, |2-3|/2=0.5, |4-5|/4=0.25 -> mean=0.583.. * 100
        result = mape([1, 2, 4], [2, 3, 5])
        assert result == pytest.approx(100 * (1 + 0.5 + 0.25) / 3)

    def test_zero_actual_raises(self):
        with pytest.raises(ValueError, match="zero"):
            mape([0, 1, 2], [1, 1, 2])


class TestSMAPE:
    def test_perfect(self):
        assert smape([1, 2, 3], [1, 2, 3]) == 0.0

    def test_symmetric(self):
        # sMAPE should be symmetric in actual / predicted
        s1 = smape([1, 2, 3], [3, 2, 1])
        s2 = smape([3, 2, 1], [1, 2, 3])
        assert s1 == pytest.approx(s2)

    def test_both_zero(self):
        # When both are zero, contribution is 0
        assert smape([0, 0], [0, 0]) == 0.0


# ---------------------------------------------------------------------------
# MASE
# ---------------------------------------------------------------------------


class TestMASE:
    def test_beats_naive(self):
        # Linear series: naive forecast is perfect for constant differences
        actual = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
        predicted = np.array([1.1, 2.1, 3.1, 4.1, 5.1])
        result = mase(actual, predicted, seasonal_period=1)
        assert result == pytest.approx(0.1)

    def test_constant_series_raises(self):
        with pytest.raises(ValueError, match="zero"):
            mase([5, 5, 5, 5], [5, 5, 5, 5], seasonal_period=1)

    def test_short_series_raises(self):
        with pytest.raises(ValueError, match="exceed"):
            mase([1, 2], [1, 2], seasonal_period=5)


# ---------------------------------------------------------------------------
# R-squared
# ---------------------------------------------------------------------------


class TestRSquared:
    def test_perfect(self):
        assert r_squared([1, 2, 3], [1, 2, 3]) == 1.0

    def test_mean_prediction(self):
        # Predicting the mean gives R^2 = 0
        a = [1.0, 2.0, 3.0]
        p = [2.0, 2.0, 2.0]
        assert r_squared(a, p) == pytest.approx(0.0)

    def test_negative_r2(self):
        # Worse than mean prediction
        r2 = r_squared([1, 2, 3], [10, 20, 30])
        assert r2 < 0

    def test_constant_actual(self):
        # All actual values equal -> ss_tot = 0
        assert r_squared([5, 5, 5], [5, 5, 5]) == 1.0


# ---------------------------------------------------------------------------
# Input validation
# ---------------------------------------------------------------------------


class TestValidation:
    def test_shape_mismatch(self):
        with pytest.raises(ValueError, match="Shape"):
            mae([1, 2], [1, 2, 3])

    def test_empty_raises(self):
        with pytest.raises(ValueError, match="empty"):
            mae([], [])

    def test_numpy_arrays_accepted(self):
        a = np.array([1.0, 2.0, 3.0])
        p = np.array([1.5, 2.5, 3.5])
        assert mae(a, p) == pytest.approx(0.5)
