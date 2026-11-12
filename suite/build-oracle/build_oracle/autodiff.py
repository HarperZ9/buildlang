"""
Automatic differentiation via dual numbers (forward mode).

A :class:`Dual` number carries both a value and its derivative with
respect to some variable of interest.  Arithmetic and elementary
functions propagate derivatives automatically via the chain rule.
"""

from __future__ import annotations

import math
from collections.abc import Callable
from typing import Union

import numpy as np

Numeric = Union[int, float, "Dual"]


class Dual:
    """Dual number for forward-mode automatic differentiation.

    ``Dual(value, derivative)`` represents a quantity whose value is
    *value* and whose derivative with respect to the tracked variable
    is *derivative*.

    Examples
    --------
    >>> x = Dual.variable(3.0)
    >>> f = x ** 2 + Dual.sin(x)
    >>> f.value   # 9 + sin(3) ≈ 9.1411
    >>> f.deriv   # 2*3 + cos(3) ≈ 5.01
    """

    __slots__ = ("value", "deriv")

    def __init__(self, value: float, derivative: float = 0.0):
        self.value = float(value)
        self.deriv = float(derivative)

    @staticmethod
    def variable(value: float) -> Dual:
        """Create a Dual that tracks its own derivative (d/dx x = 1)."""
        return Dual(value, 1.0)

    @staticmethod
    def constant(value: float) -> Dual:
        """Create a Dual constant (derivative = 0)."""
        return Dual(value, 0.0)

    # ------------------------------------------------------------------
    # Arithmetic operators
    # ------------------------------------------------------------------

    def __add__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        return Dual(self.value + other.value, self.deriv + other.deriv)

    def __radd__(self, other: Numeric) -> Dual:
        return self.__add__(other)

    def __sub__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        return Dual(self.value - other.value, self.deriv - other.deriv)

    def __rsub__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        return Dual(other.value - self.value, other.deriv - self.deriv)

    def __mul__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        # Product rule: (fg)' = f'g + fg'
        return Dual(
            self.value * other.value,
            self.deriv * other.value + self.value * other.deriv,
        )

    def __rmul__(self, other: Numeric) -> Dual:
        return self.__mul__(other)

    def __truediv__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        if other.value == 0:
            raise ZeroDivisionError("Division by zero in Dual arithmetic")
        # Quotient rule: (f/g)' = (f'g - fg') / g^2
        return Dual(
            self.value / other.value,
            (self.deriv * other.value - self.value * other.deriv) / (other.value**2),
        )

    def __rtruediv__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        return other.__truediv__(self)

    def __pow__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        # f(x)^g(x) = exp(g * ln(f))
        # d/dx = f^g * (g' * ln(f) + g * f' / f)
        val = self.value**other.value
        if self.value > 0:
            deriv = val * (other.deriv * math.log(self.value) + other.value * self.deriv / self.value)
        elif self.value == 0:
            deriv = 0.0
        else:
            # Negative base with non-integer exponent is problematic
            deriv = val * other.value * self.deriv / self.value if self.value != 0 else 0.0
        return Dual(val, deriv)

    def __rpow__(self, other: Numeric) -> Dual:
        other = _as_dual(other)
        return other.__pow__(self)

    def __neg__(self) -> Dual:
        return Dual(-self.value, -self.deriv)

    def __pos__(self) -> Dual:
        return Dual(self.value, self.deriv)

    def __abs__(self) -> Dual:
        if self.value > 0:
            return Dual(self.value, self.deriv)
        elif self.value < 0:
            return Dual(-self.value, -self.deriv)
        else:
            return Dual(0.0, 0.0)

    # ------------------------------------------------------------------
    # Comparison (on value only)
    # ------------------------------------------------------------------

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Dual):
            return self.value == other.value
        return self.value == other

    def __lt__(self, other: Numeric) -> bool:
        other = _as_dual(other)
        return self.value < other.value

    def __le__(self, other: Numeric) -> bool:
        other = _as_dual(other)
        return self.value <= other.value

    def __gt__(self, other: Numeric) -> bool:
        other = _as_dual(other)
        return self.value > other.value

    def __ge__(self, other: Numeric) -> bool:
        other = _as_dual(other)
        return self.value >= other.value

    # ------------------------------------------------------------------
    # Elementary functions (static methods for composability)
    # ------------------------------------------------------------------

    @staticmethod
    def sin(x: Numeric) -> Dual:
        x = _as_dual(x)
        return Dual(math.sin(x.value), x.deriv * math.cos(x.value))

    @staticmethod
    def cos(x: Numeric) -> Dual:
        x = _as_dual(x)
        return Dual(math.cos(x.value), -x.deriv * math.sin(x.value))

    @staticmethod
    def exp(x: Numeric) -> Dual:
        x = _as_dual(x)
        e = math.exp(x.value)
        return Dual(e, x.deriv * e)

    @staticmethod
    def log(x: Numeric) -> Dual:
        x = _as_dual(x)
        if x.value <= 0:
            raise ValueError("log requires positive value")
        return Dual(math.log(x.value), x.deriv / x.value)

    @staticmethod
    def sqrt(x: Numeric) -> Dual:
        x = _as_dual(x)
        if x.value < 0:
            raise ValueError("sqrt requires non-negative value")
        s = math.sqrt(x.value)
        deriv = x.deriv / (2.0 * s) if s != 0 else 0.0
        return Dual(s, deriv)

    @staticmethod
    def tanh(x: Numeric) -> Dual:
        x = _as_dual(x)
        t = math.tanh(x.value)
        return Dual(t, x.deriv * (1.0 - t * t))

    @staticmethod
    def sigmoid(x: Numeric) -> Dual:
        x = _as_dual(x)
        s = 1.0 / (1.0 + math.exp(-x.value))
        return Dual(s, x.deriv * s * (1.0 - s))

    @staticmethod
    def relu(x: Numeric) -> Dual:
        x = _as_dual(x)
        if x.value > 0:
            return Dual(x.value, x.deriv)
        else:
            return Dual(0.0, 0.0)

    # ------------------------------------------------------------------
    # Repr
    # ------------------------------------------------------------------

    def __repr__(self) -> str:
        return f"Dual(value={self.value}, deriv={self.deriv})"


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------


def _as_dual(x: Numeric) -> Dual:
    if isinstance(x, Dual):
        return x
    return Dual(float(x), 0.0)


# ---------------------------------------------------------------------------
# Gradient computation
# ---------------------------------------------------------------------------


def gradient(fn: Callable, x: np.ndarray) -> np.ndarray:
    """Compute the gradient of *fn* at point *x* using forward-mode AD.

    Evaluates *fn* once per dimension of *x* (each time setting one
    component's derivative to 1 and the rest to 0).

    Parameters
    ----------
    fn : callable
        A function that accepts a list/array of :class:`Dual` numbers and
        returns a single :class:`Dual`.
    x : np.ndarray
        1-D array — the point at which to evaluate the gradient.

    Returns
    -------
    np.ndarray — gradient vector of the same shape as *x*.
    """
    x = np.asarray(x, dtype=np.float64).ravel()
    n = len(x)
    grad = np.zeros(n)

    for i in range(n):
        duals = [Dual(x[j], 1.0 if j == i else 0.0) for j in range(n)]
        result = fn(duals)
        if isinstance(result, Dual):
            grad[i] = result.deriv
        else:
            grad[i] = 0.0

    return grad
