"""Tests for build_oracle.neural — layers, backprop, and SimpleForecaster."""

import numpy as np
import pytest

from build_oracle.neural import Linear, ReLU, SimpleForecaster

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _sin_series(n: int = 200, seed: int = 42) -> np.ndarray:
    """Sine wave with slight noise — easy for an MLP to learn."""
    rng = np.random.default_rng(seed)
    return np.sin(np.arange(n) * 0.1) * 5 + 50 + rng.normal(0, 0.5, n)


def _constant_series(n: int = 200, value: float = 42.0) -> np.ndarray:
    return np.full(n, value, dtype=np.float64)


# ---------------------------------------------------------------------------
# Linear layer backward
# ---------------------------------------------------------------------------


class TestLinearBackward:
    def test_forward_caches_input(self):
        layer = Linear(4, 3)
        x = np.random.randn(2, 4)
        layer.forward(x)
        assert layer._input is x

    def test_backward_gradient_shapes(self):
        layer = Linear(4, 3)
        x = np.random.randn(5, 4)
        layer.forward(x)
        d_out = np.random.randn(5, 3)
        d_input = layer._backward(d_out)
        assert layer._dW.shape == (4, 3)
        assert layer._db.shape == (3,)
        assert d_input.shape == (5, 4)

    def test_backward_numerical_gradient(self):
        """Finite-difference check for Linear._backward."""
        layer = Linear(3, 2)
        x = np.random.randn(4, 3)
        layer.forward(x)
        d_out = np.random.randn(4, 2)
        layer._backward(d_out)

        # Numerical gradient for W
        eps = 1e-5
        num_dW = np.zeros_like(layer.W)
        for i in range(layer.W.shape[0]):
            for j in range(layer.W.shape[1]):
                layer.W[i, j] += eps
                out_plus = x @ layer.W + layer.b
                layer.W[i, j] -= 2 * eps
                out_minus = x @ layer.W + layer.b
                layer.W[i, j] += eps  # restore
                num_dW[i, j] = np.sum(d_out * (out_plus - out_minus)) / (2 * eps)
        np.testing.assert_allclose(layer._dW, num_dW, atol=1e-5)


# ---------------------------------------------------------------------------
# ReLU backward
# ---------------------------------------------------------------------------


class TestReLUBackward:
    def test_forward_caches_input(self):
        relu = ReLU()
        x = np.array([-1.0, 0.0, 1.0, 2.0])
        relu.forward(x)
        np.testing.assert_array_equal(relu._input, x)

    def test_backward_masks_negative(self):
        relu = ReLU()
        x = np.array([-2.0, -1.0, 0.0, 1.0, 3.0])
        relu.forward(x)
        d_out = np.ones_like(x)
        grad = relu._backward(d_out)
        expected = np.array([0.0, 0.0, 0.0, 1.0, 1.0])
        np.testing.assert_array_equal(grad, expected)


# ---------------------------------------------------------------------------
# SimpleForecaster construction
# ---------------------------------------------------------------------------


class TestForecasterConstruction:
    def test_default_params(self):
        fc = SimpleForecaster()
        assert fc.lookback == 20
        assert fc.horizon == 5
        assert fc.hidden == 64

    def test_custom_params(self):
        fc = SimpleForecaster(lookback=10, horizon=3, hidden=16)
        assert fc.lookback == 10
        assert fc.horizon == 3
        assert fc.hidden == 16

    def test_repr(self):
        assert "SimpleForecaster" in repr(SimpleForecaster())


# ---------------------------------------------------------------------------
# Training
# ---------------------------------------------------------------------------


class TestForecasterTrain:
    def test_training_reduces_loss(self):
        """Core test: training on a sine wave must reduce MSE."""
        np.random.seed(0)
        series = _sin_series(200)
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=32)
        losses = fc.train(series, epochs=200, lr=0.001)
        assert losses[-1] < losses[0], "Training should reduce loss"

    def test_loss_history_length(self):
        series = _sin_series(200)
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=16)
        losses = fc.train(series, epochs=50, lr=0.001)
        assert len(losses) == 50

    def test_trained_flag_set(self):
        series = _sin_series(200)
        fc = SimpleForecaster(lookback=10, horizon=3, hidden=8)
        fc.train(series, epochs=10, lr=0.001)
        assert fc._trained is True

    def test_trained_model_beats_random(self):
        """Trained model should have lower MSE than freshly initialised one."""
        np.random.seed(1)
        series = _sin_series(300, seed=7)
        train_data = series[:250]
        test_input = series[240:260]  # 20-length window
        test_target = series[260:265]  # 5-length target

        # Untrained model
        fc_random = SimpleForecaster(lookback=20, horizon=5, hidden=32)
        pred_random = fc_random.predict(test_input)
        mse_random = float(np.mean((pred_random - test_target) ** 2))

        # Trained model
        fc_trained = SimpleForecaster(lookback=20, horizon=5, hidden=32)
        fc_trained.train(train_data, epochs=300, lr=0.001)
        pred_trained = fc_trained.predict(test_input)
        mse_trained = float(np.mean((pred_trained - test_target) ** 2))

        assert mse_trained < mse_random, f"Trained MSE ({mse_trained:.4f}) should be < random MSE ({mse_random:.4f})"

    def test_constant_series_converges(self):
        """A constant series should drive loss very close to zero."""
        series = _constant_series(100, value=42.0)
        fc = SimpleForecaster(lookback=10, horizon=3, hidden=8)
        losses = fc.train(series, epochs=200, lr=0.001)
        assert losses[-1] < 0.01, f"Constant-series loss should be tiny, got {losses[-1]}"


# ---------------------------------------------------------------------------
# fit() alias
# ---------------------------------------------------------------------------


class TestFitAlias:
    def test_fit_returns_same_as_train(self):
        """fit() should behave identically to train()."""
        series = _sin_series(200)

        np.random.seed(99)
        fc1 = SimpleForecaster(lookback=10, horizon=3, hidden=8)
        losses_train = fc1.train(series, epochs=30, lr=0.001)

        np.random.seed(99)
        fc2 = SimpleForecaster(lookback=10, horizon=3, hidden=8)
        losses_fit = fc2.fit(series, epochs=30, lr=0.001)

        # Both should return a list of losses and set _trained
        assert fc2._trained is True
        assert len(losses_fit) == len(losses_train)


# ---------------------------------------------------------------------------
# Prediction
# ---------------------------------------------------------------------------


class TestForecasterPredict:
    def test_predict_shape(self):
        series = _sin_series(200)
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=16)
        fc.train(series, epochs=10, lr=0.001)
        pred = fc.predict(series[-30:])
        assert pred.shape == (5,)

    def test_predict_finite(self):
        series = _sin_series(200)
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=16)
        fc.train(series, epochs=50, lr=0.001)
        pred = fc.predict(series[-20:])
        assert np.all(np.isfinite(pred))

    def test_predict_without_training(self):
        """predict() should still work on an untrained model (raw weights)."""
        fc = SimpleForecaster(lookback=10, horizon=3, hidden=8)
        pred = fc.predict(np.arange(10, dtype=float))
        assert pred.shape == (3,)


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    def test_series_too_short_for_train(self):
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=8)
        with pytest.raises(ValueError, match="must be >="):
            fc.train(np.arange(10, dtype=float), epochs=5)

    def test_series_too_short_for_predict(self):
        fc = SimpleForecaster(lookback=20, horizon=5, hidden=8)
        with pytest.raises(ValueError, match="must be >= lookback"):
            fc.predict(np.arange(5, dtype=float))

    def test_single_window_trains(self):
        """Minimum viable series: exactly lookback + horizon + 1 values."""
        fc = SimpleForecaster(lookback=5, horizon=2, hidden=4)
        series = np.arange(8, dtype=float)  # 5 + 2 + 1 = 8
        losses = fc.train(series, epochs=10, lr=0.01)
        assert len(losses) == 10
