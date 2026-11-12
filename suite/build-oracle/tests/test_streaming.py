"""Tests for build_oracle.streaming."""

import numpy as np

from build_oracle.streaming import StreamConfig, StreamForecaster, StreamUpdate

# ---------------------------------------------------------------------------
# Helpers -- synthetic data generators
# ---------------------------------------------------------------------------


def _sine_stream(n: int = 200, period: float = 20.0, seed: int = 42) -> np.ndarray:
    """Sine wave with light noise."""
    rng = np.random.default_rng(seed)
    t = np.arange(n, dtype=np.float64)
    return 10.0 + 3.0 * np.sin(2 * np.pi * t / period) + rng.normal(0, 0.3, n)


def _linear_stream(n: int = 200, seed: int = 42) -> np.ndarray:
    """Linear trend with noise."""
    rng = np.random.default_rng(seed)
    return 5.0 + 0.05 * np.arange(n, dtype=np.float64) + rng.normal(0, 0.2, n)


def _fast_config(**overrides) -> StreamConfig:
    """Config tuned for fast test runs (fewer epochs, smaller nets)."""
    defaults = dict(
        window_size=100,
        refit_interval=50,
        min_history=20,
        models=["arima", "prophet"],  # skip neural by default for speed
        forecast_horizon=5,
        decay_factor=0.95,
        neural_epochs=10,
        neural_hidden=16,
        neural_lookback=10,
    )
    defaults.update(overrides)
    return StreamConfig(**defaults)


# ---------------------------------------------------------------------------
# StreamConfig tests
# ---------------------------------------------------------------------------


class TestStreamConfig:
    def test_defaults(self):
        cfg = StreamConfig()
        assert cfg.window_size == 200
        assert cfg.refit_interval == 50
        assert cfg.min_history == 30
        assert cfg.forecast_horizon == 5
        assert cfg.decay_factor == 0.95
        assert "arima" in cfg.models
        assert "prophet" in cfg.models
        assert "neural" in cfg.models

    def test_custom_config(self):
        cfg = StreamConfig(
            window_size=100,
            min_history=10,
            models=["arima"],
        )
        assert cfg.window_size == 100
        assert cfg.min_history == 10
        assert cfg.models == ["arima"]


# ---------------------------------------------------------------------------
# Warm-up period tests
# ---------------------------------------------------------------------------


class TestWarmUp:
    def test_returns_none_during_warmup(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        # First 19 points should return None
        for i in range(19):
            result = sf.observe(data[i])
            assert result is None, f"Expected None at index {i}, got {type(result)}"

    def test_history_grows_during_warmup(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(30)

        for i in range(15):
            sf.observe(data[i])
        assert sf.history_length == 15
        assert not sf.is_ready

    def test_is_ready_after_min_history(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(30)

        for i in range(20):
            sf.observe(data[i])
        assert sf.is_ready


# ---------------------------------------------------------------------------
# First prediction after warm-up
# ---------------------------------------------------------------------------


class TestFirstPrediction:
    def test_first_update_returns_stream_update(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        result = None
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                break

        assert result is not None
        assert isinstance(result, StreamUpdate)
        assert result.observed == data[cfg.min_history - 1]

    def test_first_update_has_predictions(self):
        cfg = _fast_config(min_history=20, forecast_horizon=5)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        result = None
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                break

        assert result is not None
        assert len(result.next_predictions) == 5
        assert np.all(np.isfinite(result.next_predictions))

    def test_first_update_has_model_weights(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        result = None
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                break

        assert result is not None
        assert len(result.model_weights) > 0
        total = sum(result.model_weights.values())
        assert abs(total - 1.0) < 1e-8

    def test_first_update_confidence_between_0_and_1(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        result = None
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                break

        assert result is not None
        assert 0.0 <= result.confidence <= 1.0


# ---------------------------------------------------------------------------
# Continuous stream processing
# ---------------------------------------------------------------------------


class TestContinuousStream:
    def test_100_point_sine_all_updates_valid(self):
        """Process 100 points of a sine wave; all post-warmup updates valid."""
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(100)

        updates = []
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                updates.append(result)

        # Should have gotten updates for all points after warmup
        assert len(updates) == 100 - cfg.min_history + 1

        # All predictions should be finite
        for u in updates:
            assert np.all(np.isfinite(u.next_predictions))
            assert np.isfinite(u.error)
            assert 0.0 <= u.confidence <= 1.0

    def test_predictions_are_correct_length(self):
        cfg = _fast_config(min_history=20, forecast_horizon=7)
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None:
                assert len(result.next_predictions) == 7

    def test_history_trimmed_to_window_size(self):
        cfg = _fast_config(window_size=50, min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(100)

        for v in data:
            sf.observe(v)

        assert sf.history_length <= cfg.window_size


# ---------------------------------------------------------------------------
# Accuracy improvement over time
# ---------------------------------------------------------------------------


class TestAccuracyImprovement:
    def test_later_errors_not_worse_than_initial(self):
        """Errors in the second half should not be dramatically worse
        than the first half, indicating the model is tracking."""
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(120)

        errors = []
        for i in range(len(data)):
            result = sf.observe(data[i])
            if result is not None and result.error > 0:
                errors.append(result.error)

        if len(errors) > 20:
            first_half = errors[: len(errors) // 2]
            second_half = errors[len(errors) // 2 :]
            avg_first = np.mean(first_half)
            avg_second = np.mean(second_half)
            # Second half should not be 5x worse than first
            assert avg_second < avg_first * 5.0, f"Second half error {avg_second:.4f} >> first half {avg_first:.4f}"


# ---------------------------------------------------------------------------
# Refit triggers
# ---------------------------------------------------------------------------


class TestRefit:
    def test_refit_triggers_at_interval(self):
        cfg = _fast_config(
            min_history=20,
            refit_interval=25,
            models=["arima"],
        )
        sf = StreamForecaster(cfg)
        data = _sine_stream(100)

        for v in data:
            sf.observe(v)

        # After initial fit at point 20, we get incremental updates.
        # Refit should happen at update_count multiples of 25.
        # With 80 updates (points 21-100), refits at 25, 50, 75 = 3
        assert sf.refit_count >= 1

    def test_update_count_increments(self):
        cfg = _fast_config(min_history=20, models=["arima"])
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        for v in data:
            sf.observe(v)

        # update_count tracks incremental updates after initial fit
        assert sf.update_count == 50 - cfg.min_history


# ---------------------------------------------------------------------------
# Weight updates
# ---------------------------------------------------------------------------


class TestWeightUpdates:
    def test_weights_sum_to_one(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        last_result = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last_result = result

        assert last_result is not None
        total = sum(last_result.model_weights.values())
        assert abs(total - 1.0) < 1e-8

    def test_weights_all_positive(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        last_result = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last_result = result

        assert last_result is not None
        for name, w in last_result.model_weights.items():
            assert w > 0, f"Weight for {name} is {w}, expected > 0"

    def test_weights_evolve_over_time(self):
        """Weights should change as more data is observed."""
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(80)

        first_weights = None
        last_weights = None

        for _i, v in enumerate(data):
            result = sf.observe(v)
            if result is not None:
                if first_weights is None:
                    first_weights = dict(result.model_weights)
                last_weights = dict(result.model_weights)

        # At least one model's weight should have changed
        assert first_weights is not None
        assert last_weights is not None
        changed = any(
            abs(first_weights.get(k, 0) - last_weights.get(k, 0)) > 1e-10
            for k in set(first_weights) | set(last_weights)
        )
        assert changed, "Weights did not change over 80 observations"


# ---------------------------------------------------------------------------
# Reset
# ---------------------------------------------------------------------------


class TestReset:
    def test_reset_clears_state(self):
        cfg = _fast_config(min_history=20)
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        for v in data:
            sf.observe(v)

        assert sf.is_ready
        assert sf.history_length > 0

        sf.reset()

        assert sf.history_length == 0
        assert not sf.is_ready
        assert not sf._fitted
        assert sf._update_count == 0
        assert len(sf._models) == 0
        assert len(sf._errors) == 0
        assert len(sf._weights) == 0
        assert sf._last_prediction is None
        assert sf._refit_count == 0

    def test_can_reuse_after_reset(self):
        cfg = _fast_config(min_history=20, models=["arima"])
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        for v in data:
            sf.observe(v)
        sf.reset()

        # Feed new data after reset
        new_data = _linear_stream(40)
        last_result = None
        for v in new_data:
            result = sf.observe(v)
            if result is not None:
                last_result = result

        assert last_result is not None
        assert np.all(np.isfinite(last_result.next_predictions))


# ---------------------------------------------------------------------------
# Different configurations
# ---------------------------------------------------------------------------


class TestDifferentConfigs:
    def test_arima_only(self):
        cfg = _fast_config(min_history=20, models=["arima"])
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert "arima" in last.model_weights

    def test_prophet_only(self):
        cfg = _fast_config(min_history=20, models=["prophet"])
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert "prophet" in last.model_weights

    def test_with_neural(self):
        cfg = _fast_config(
            min_history=30,
            models=["neural"],
            neural_epochs=10,
            neural_hidden=16,
            neural_lookback=10,
        )
        sf = StreamForecaster(cfg)
        data = _sine_stream(80)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert "neural" in last.model_weights

    def test_all_three_models(self):
        cfg = _fast_config(
            min_history=30,
            models=["arima", "prophet", "neural"],
            neural_epochs=10,
            neural_hidden=16,
            neural_lookback=10,
        )
        sf = StreamForecaster(cfg)
        data = _sine_stream(80)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert len(last.model_weights) >= 2  # at least 2 should succeed

    def test_small_window_size(self):
        cfg = _fast_config(
            window_size=30,
            min_history=15,
            models=["arima"],
        )
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        for v in data:
            sf.observe(v)

        assert sf.history_length <= 30

    def test_large_forecast_horizon(self):
        cfg = _fast_config(
            min_history=20,
            forecast_horizon=20,
            models=["arima"],
        )
        sf = StreamForecaster(cfg)
        data = _sine_stream(50)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert len(last.next_predictions) == 20

    def test_custom_decay_factor(self):
        cfg = _fast_config(min_history=20, decay_factor=0.5)
        sf = StreamForecaster(cfg)
        data = _sine_stream(60)

        last = None
        for v in data:
            result = sf.observe(v)
            if result is not None:
                last = result

        assert last is not None
        assert abs(sum(last.model_weights.values()) - 1.0) < 1e-8


# ---------------------------------------------------------------------------
# Repr
# ---------------------------------------------------------------------------


class TestRepr:
    def test_unfitted_repr(self):
        sf = StreamForecaster(_fast_config(min_history=20))
        r = repr(sf)
        assert "warming_up" in r
        assert "0/20" in r

    def test_fitted_repr(self):
        cfg = _fast_config(min_history=20, models=["arima"])
        sf = StreamForecaster(cfg)
        data = _sine_stream(30)

        for v in data:
            sf.observe(v)

        r = repr(sf)
        assert "StreamForecaster" in r
        assert "arima" in r
