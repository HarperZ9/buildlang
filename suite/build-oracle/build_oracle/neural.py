"""
Simple neural network layers built on NumPy (no PyTorch dependency).

Provides Linear, LayerNorm, activation functions, an LSTM cell,
and a lightweight MLP forecaster.
"""

from __future__ import annotations

import math
from typing import Any

import numpy as np

JSONDict = dict[str, Any]

# ---------------------------------------------------------------------------
# Weight initialisation helpers
# ---------------------------------------------------------------------------


def _kaiming_uniform(fan_in: int, fan_out: int) -> np.ndarray:
    """Kaiming (He) uniform initialisation for ReLU networks."""
    limit = math.sqrt(6.0 / fan_in)
    return np.random.uniform(-limit, limit, size=(fan_in, fan_out))


def _xavier_uniform(fan_in: int, fan_out: int) -> np.ndarray:
    """Xavier (Glorot) uniform initialisation."""
    limit = math.sqrt(6.0 / (fan_in + fan_out))
    return np.random.uniform(-limit, limit, size=(fan_in, fan_out))


# ---------------------------------------------------------------------------
# Linear (Dense) layer
# ---------------------------------------------------------------------------


class Linear:
    """Fully-connected (dense) layer: ``z = x @ W + b``.

    Parameters
    ----------
    in_features : int
        Number of input features.
    out_features : int
        Number of output features.
    """

    def __init__(self, in_features: int, out_features: int):
        self.in_features = in_features
        self.out_features = out_features
        self.W = _kaiming_uniform(in_features, out_features)
        self.b = np.zeros(out_features)
        # Cached for backward pass
        self._input: np.ndarray | None = None
        self._dW: np.ndarray | None = None
        self._db: np.ndarray | None = None

    def forward(self, x: np.ndarray) -> np.ndarray:
        """Compute the linear transformation.

        Parameters
        ----------
        x : (..., in_features) array

        Returns
        -------
        (..., out_features) array
        """
        self._input = x
        return x @ self.W + self.b

    def _backward(self, d_output: np.ndarray) -> np.ndarray:
        """Compute gradients and return upstream gradient.

        Parameters
        ----------
        d_output : (..., out_features) array
            Gradient of loss w.r.t. this layer's output.

        Returns
        -------
        (..., in_features) array — gradient w.r.t. this layer's input.
        """
        if self._input is None:
            raise RuntimeError("forward() must be called before _backward()")
        self._dW = self._input.T @ d_output
        self._db = d_output.sum(axis=0)
        return d_output @ self.W.T

    def sgd_step(self, lr: float) -> None:
        """Apply a gradient-descent update using the last computed gradients."""
        if self._dW is None or self._db is None:
            raise RuntimeError("_backward() must be called before sgd_step()")
        self.W -= lr * self._dW
        self.b -= lr * self._db

    def parameters(self) -> list[np.ndarray]:
        """Return a list of trainable parameter arrays [W, b]."""
        return [self.W, self.b]

    def __repr__(self) -> str:
        return f"Linear({self.in_features}, {self.out_features})"


# ---------------------------------------------------------------------------
# Layer Normalization
# ---------------------------------------------------------------------------


class LayerNorm:
    """Layer normalization over the last dimension.

    ``y = (x - mean) / sqrt(var + eps) * gamma + beta``

    Parameters
    ----------
    features : int
        Size of the last dimension.
    eps : float
        Small constant for numerical stability.
    """

    def __init__(self, features: int, eps: float = 1e-5):
        self.features = features
        self.eps = eps
        self.gamma = np.ones(features)
        self.beta = np.zeros(features)

    def forward(self, x: np.ndarray) -> np.ndarray:
        mean = np.mean(x, axis=-1, keepdims=True)
        var = np.var(x, axis=-1, keepdims=True)
        x_norm = (x - mean) / np.sqrt(var + self.eps)
        return self.gamma * x_norm + self.beta

    def parameters(self) -> list[np.ndarray]:
        return [self.gamma, self.beta]

    def __repr__(self) -> str:
        return f"LayerNorm({self.features})"


# ---------------------------------------------------------------------------
# Activation functions
# ---------------------------------------------------------------------------


class ReLU:
    """Rectified Linear Unit activation."""

    def __init__(self):
        self._input: np.ndarray | None = None

    def forward(self, x: np.ndarray) -> np.ndarray:
        self._input = x
        return np.maximum(0.0, x)

    def _backward(self, d_output: np.ndarray) -> np.ndarray:
        """Mask gradient where input was <= 0."""
        if self._input is None:
            raise RuntimeError("forward() must be called before _backward()")
        return d_output * (self._input > 0)

    def __repr__(self) -> str:
        return "ReLU()"


class Sigmoid:
    """Logistic sigmoid activation."""

    def forward(self, x: np.ndarray) -> np.ndarray:
        # Numerically stable sigmoid
        pos = x >= 0
        z = np.zeros_like(x)
        z[pos] = 1.0 / (1.0 + np.exp(-x[pos]))
        exp_x = np.exp(x[~pos])
        z[~pos] = exp_x / (1.0 + exp_x)
        return z

    def __repr__(self) -> str:
        return "Sigmoid()"


class Tanh:
    """Hyperbolic tangent activation."""

    def forward(self, x: np.ndarray) -> np.ndarray:
        return np.tanh(x)

    def __repr__(self) -> str:
        return "Tanh()"


# ---------------------------------------------------------------------------
# LSTM Cell
# ---------------------------------------------------------------------------


class LSTMCell:
    """Single Long Short-Term Memory cell.

    Gate equations (concatenated input ``[x, h_prev]``):

    - Forget gate:  f = sigmoid(W_f @ [x, h] + b_f)
    - Input gate:   i = sigmoid(W_i @ [x, h] + b_i)
    - Candidate:    g = tanh(W_g @ [x, h] + b_g)
    - Output gate:  o = sigmoid(W_o @ [x, h] + b_o)
    - Cell state:   c = f * c_prev + i * g
    - Hidden state: h = o * tanh(c)

    Parameters
    ----------
    input_size : int
        Dimensionality of the input vector ``x``.
    hidden_size : int
        Dimensionality of the hidden / cell state.
    """

    def __init__(self, input_size: int, hidden_size: int):
        self.input_size = input_size
        self.hidden_size = hidden_size

        combined = input_size + hidden_size

        # Weights for all four gates, stacked: [forget, input, candidate, output]
        self.W = _xavier_uniform(combined, 4 * hidden_size)
        self.b = np.zeros(4 * hidden_size)

        self._sigmoid = Sigmoid()

    def forward(
        self,
        x: np.ndarray,
        h_prev: np.ndarray,
        c_prev: np.ndarray,
    ) -> tuple[np.ndarray, np.ndarray]:
        """Run one LSTM step.

        Parameters
        ----------
        x : (input_size,) array — current input.
        h_prev : (hidden_size,) array — previous hidden state.
        c_prev : (hidden_size,) array — previous cell state.

        Returns
        -------
        (h_next, c_next) — updated hidden and cell states.
        """
        combined = np.concatenate([x, h_prev])
        gates = combined @ self.W + self.b

        hs = self.hidden_size
        f_gate = self._sigmoid.forward(gates[0 * hs : 1 * hs])
        i_gate = self._sigmoid.forward(gates[1 * hs : 2 * hs])
        g_gate = np.tanh(gates[2 * hs : 3 * hs])
        o_gate = self._sigmoid.forward(gates[3 * hs : 4 * hs])

        c_next = f_gate * c_prev + i_gate * g_gate
        h_next = o_gate * np.tanh(c_next)

        return h_next, c_next

    def parameters(self) -> list[np.ndarray]:
        return [self.W, self.b]

    def __repr__(self) -> str:
        return f"LSTMCell({self.input_size}, {self.hidden_size})"


# ---------------------------------------------------------------------------
# Simple MLP Forecaster
# ---------------------------------------------------------------------------


class SimpleForecaster:
    """Two-layer MLP for univariate or multivariate time series forecasting.

    Takes a lookback window of past values and produces *horizon*
    future predictions.  When the input series is 2-D (T, K), the
    input layer size is ``lookback * K`` and the output remains
    univariate (predicting the first column by default).

    Architecture::

        Input(lookback * n_vars) -> Linear -> ReLU -> Linear -> Output(horizon)

    Parameters
    ----------
    lookback : int
        Number of past observations used as input features.
    horizon : int
        Number of future steps to forecast.
    hidden : int
        Width of the hidden layer.
    n_vars : int
        Number of input variables.  1 = univariate (default).
    """

    def __init__(self, lookback: int = 20, horizon: int = 5, hidden: int = 64, n_vars: int = 1):
        self.lookback = lookback
        self.horizon = horizon
        self.hidden = hidden
        self.n_vars = n_vars

        input_size = lookback * n_vars
        self.layer1 = Linear(input_size, hidden)
        self.relu = ReLU()
        self.layer2 = Linear(hidden, horizon)

        # Normalization stats (set during training)
        self._train_mean: float = 0.0
        self._train_std: float = 1.0
        self._trained: bool = False

    # ------------------------------------------------------------------
    # Forward helpers
    # ------------------------------------------------------------------

    def _forward(self, x: np.ndarray) -> np.ndarray:
        """Full forward pass through the network.

        Parameters
        ----------
        x : (batch, lookback) array

        Returns
        -------
        (batch, horizon) array
        """
        z = self.layer1.forward(x)
        z = self.relu.forward(z)
        z = self.layer2.forward(z)
        return z

    # ------------------------------------------------------------------
    # Training
    # ------------------------------------------------------------------

    def train(
        self,
        series: np.ndarray,
        epochs: int = 100,
        lr: float = 0.001,
    ) -> list[float]:
        """Train the forecaster on a univariate or multivariate time series.

        Creates sliding windows of length ``lookback + horizon`` from
        *series*, normalizes them to zero-mean / unit-variance, and runs
        mini-batch SGD (full-batch per epoch) with back-propagation.

        For multivariate input (T, K), the target is the first column and
        each input window is flattened to ``lookback * K`` features.

        Parameters
        ----------
        series : 1-D or 2-D array
            Training time series.  Shape (T,) for univariate or (T, K) for
            multivariate.  Length T must be >= lookback + horizon + 1.
        epochs : int
            Number of training iterations over the full dataset.
        lr : float
            Learning rate for SGD weight updates.

        Returns
        -------
        list[float]
            MSE loss recorded at every epoch.
        """
        series = np.asarray(series, dtype=np.float64)
        multivariate = series.ndim == 2

        if multivariate:
            if series.shape[1] != self.n_vars:
                raise ValueError(f"Expected {self.n_vars} variables, got {series.shape[1]}")
            T = series.shape[0]
            # Target is the first column
            target_col = series[:, 0]
        else:
            series = series.ravel()
            T = len(series)
            target_col = series

        min_len = self.lookback + self.horizon + 1
        if min_len > T:
            raise ValueError(f"Series length ({T}) must be >= lookback + horizon + 1 ({min_len})")

        # Compute and store normalization statistics (based on target column)
        self._train_mean = float(np.mean(target_col))
        self._train_std = float(np.std(target_col))
        if self._train_std == 0:
            self._train_std = 1.0

        if multivariate:
            normed = (series - np.mean(series, axis=0)) / (np.std(series, axis=0) + 1e-8)
            normed_target = (target_col - self._train_mean) / self._train_std
        else:
            normed = (series - self._train_mean) / self._train_std
            normed_target = normed

        # Build sliding windows ------------------------------------------------
        n_windows = T - self.lookback - self.horizon + 1
        input_size = self.lookback * self.n_vars
        X = np.zeros((n_windows, input_size))
        Y = np.zeros((n_windows, self.horizon))
        for i in range(n_windows):
            if multivariate:
                X[i] = normed[i : i + self.lookback].ravel()
            else:
                X[i] = normed[i : i + self.lookback]
            Y[i] = normed_target[i + self.lookback : i + self.lookback + self.horizon]

        # Training loop --------------------------------------------------------
        loss_history: list[float] = []
        rng = np.random.default_rng()

        for epoch in range(epochs):
            # Shuffle windows
            idx = rng.permutation(n_windows)
            X_shuf = X[idx]
            Y_shuf = Y[idx]

            # Forward pass
            output = self._forward(X_shuf)

            # MSE loss
            diff = output - Y_shuf
            loss = float(np.mean(diff**2))
            loss_history.append(loss)

            # Backward pass ----------------------------------------------------
            # d_loss / d_output  =  2 * (output - target) / N
            d_loss = 2.0 * diff / diff.size

            d_z = self.layer2._backward(d_loss)
            d_z = self.relu._backward(d_z)
            self.layer1._backward(d_z)

            # SGD update -------------------------------------------------------
            self.layer2.sgd_step(lr)
            self.layer1.sgd_step(lr)

            if epoch % 20 == 0 or epoch == epochs - 1:
                print(f"Epoch {epoch:4d}/{epochs}  loss={loss:.6f}")

        self._trained = True
        return loss_history

    def fit(
        self,
        series: np.ndarray,
        epochs: int = 100,
        lr: float = 0.001,
    ) -> list[float]:
        """Alias for :meth:`train` (matches ARIMA / Prophet API)."""
        return self.train(series, epochs=epochs, lr=lr)

    # ------------------------------------------------------------------
    # Prediction
    # ------------------------------------------------------------------

    def predict(self, series: np.ndarray) -> np.ndarray:
        """Generate a forecast from the last ``lookback`` values of *series*.

        If the model has been trained, the input is normalized using the
        training statistics and the output is denormalized back to the
        original scale.

        Parameters
        ----------
        series : 1-D array (>= lookback,) for univariate, or
                 2-D array (>= lookback, K) for multivariate.

        Returns
        -------
        (horizon,) array of predictions (first variable for multivariate).
        """
        series = np.asarray(series, dtype=np.float64)
        multivariate = series.ndim == 2

        if multivariate:
            if series.shape[0] < self.lookback:
                raise ValueError(f"Series length ({series.shape[0]}) must be >= lookback ({self.lookback})")
            window = series[-self.lookback :]  # (lookback, K)
            if self._trained:
                window = (window - np.mean(series, axis=0)) / (np.std(series, axis=0) + 1e-8)
            window_flat = window.ravel()  # (lookback * K,)
        else:
            series = series.ravel()
            if len(series) < self.lookback:
                raise ValueError(f"Series length ({len(series)}) must be >= lookback ({self.lookback})")
            window_flat = series[-self.lookback :]
            if self._trained:
                window_flat = (window_flat - self._train_mean) / self._train_std

        # Reshape to (1, input_size) for the forward pass then squeeze back
        z = self._forward(window_flat.reshape(1, -1)).ravel()

        if self._trained:
            z = z * self._train_std + self._train_mean

        return z

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def _get_state(self) -> dict:
        """Return a JSON-serializable dictionary of fitted model state."""
        return {
            "model_type": "neural",
            "lookback": self.lookback,
            "horizon": self.horizon,
            "hidden": self.hidden,
            "n_vars": self.n_vars,
            "layer1_W": self.layer1.W.tolist(),
            "layer1_b": self.layer1.b.tolist(),
            "layer2_W": self.layer2.W.tolist(),
            "layer2_b": self.layer2.b.tolist(),
            "train_mean": self._train_mean,
            "train_std": self._train_std,
            "trained": self._trained,
        }

    @classmethod
    def _from_state(cls, state: dict) -> SimpleForecaster:
        """Reconstruct a SimpleForecaster from a state dictionary."""
        if state.get("model_type") != "neural":
            raise ValueError(f"Expected model_type 'neural', got '{state.get('model_type')}'")
        obj = cls(
            lookback=state["lookback"],
            horizon=state["horizon"],
            hidden=state["hidden"],
            n_vars=state.get("n_vars", 1),
        )
        obj.layer1.W = np.array(state["layer1_W"], dtype=np.float64)
        obj.layer1.b = np.array(state["layer1_b"], dtype=np.float64)
        obj.layer2.W = np.array(state["layer2_W"], dtype=np.float64)
        obj.layer2.b = np.array(state["layer2_b"], dtype=np.float64)
        obj._train_mean = float(state["train_mean"])
        obj._train_std = float(state["train_std"])
        obj._trained = bool(state["trained"])
        return obj

    def save(self, path: str) -> None:
        """Save model to disk as JSON."""
        import json

        state = self._get_state()
        with open(path, "w") as f:
            json.dump(state, f)

    @classmethod
    def load(cls, path: str) -> SimpleForecaster:
        """Load a previously saved model from *path*."""
        import json

        with open(path) as f:
            state = json.load(f)
        return cls._from_state(state)

    def parameters(self) -> list[np.ndarray]:
        return self.layer1.parameters() + self.layer2.parameters()

    def __repr__(self) -> str:
        return f"SimpleForecaster(lookback={self.lookback}, horizon={self.horizon}, hidden={self.hidden})"
