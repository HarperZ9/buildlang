"""
Portfolio optimization algorithms.

Implements mean-variance, risk parity, Black-Litterman, and
hierarchical risk parity (HRP) portfolio construction.
"""

from __future__ import annotations

import numpy as np
from scipy import optimize
from scipy.cluster.hierarchy import linkage
from scipy.spatial.distance import squareform

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _annualize(daily_returns: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return annualized mean returns and covariance from daily returns."""
    mu = daily_returns.mean(axis=0) * 252
    cov = np.cov(daily_returns, rowvar=False) * 252
    return mu, cov


def portfolio_stats(
    weights: np.ndarray,
    returns: np.ndarray,
    risk_free: float = 0.02,
) -> dict[str, float]:
    """Portfolio return, volatility, and Sharpe for a given weight vector.

    Parameters
    ----------
    weights:
        ``(N,)`` weight array (should sum to 1).
    returns:
        ``(T, N)`` matrix of daily asset returns.
    risk_free:
        Annualized risk-free rate.

    Returns
    -------
    dict with keys ``"return"``, ``"volatility"``, ``"sharpe"``.
    """
    mu, cov = _annualize(returns)
    port_ret = float(weights @ mu)
    port_vol = float(np.sqrt(weights @ cov @ weights))
    sharpe = (port_ret - risk_free) / port_vol if port_vol > 0 else 0.0
    return {"return": port_ret, "volatility": port_vol, "sharpe": sharpe}


# ---------------------------------------------------------------------------
# Mean-variance optimization
# ---------------------------------------------------------------------------


def mean_variance_optimize(
    returns: np.ndarray,
    target: str = "max_sharpe",
    risk_free: float = 0.02,
) -> np.ndarray:
    """Classical Markowitz mean-variance optimization.

    Parameters
    ----------
    returns:
        ``(T, N)`` array of daily asset returns.
    target:
        ``"max_sharpe"`` -- maximise Sharpe ratio.
        ``"min_variance"`` -- global minimum variance portfolio.
        ``"max_return"`` -- maximise expected return (subject to full
        investment and long-only constraints).
    risk_free:
        Annualized risk-free rate for Sharpe calculation.

    Returns
    -------
    np.ndarray:
        ``(N,)`` optimal weight vector summing to 1.
    """
    n_assets = returns.shape[1]
    mu, cov = _annualize(returns)

    # Constraints: weights sum to 1
    constraints = [{"type": "eq", "fun": lambda w: np.sum(w) - 1.0}]
    # Bounds: long-only
    bounds = [(0.0, 1.0)] * n_assets
    x0 = np.ones(n_assets) / n_assets

    if target == "max_sharpe":

        def neg_sharpe(w: np.ndarray) -> float:
            p_ret = w @ mu
            p_vol = np.sqrt(w @ cov @ w)
            return -(p_ret - risk_free) / p_vol if p_vol > 1e-12 else 0.0

        result = optimize.minimize(
            neg_sharpe,
            x0,
            method="SLSQP",
            bounds=bounds,
            constraints=constraints,
            options={"maxiter": 1000, "ftol": 1e-12},
        )

    elif target == "min_variance":

        def variance(w: np.ndarray) -> float:
            return float(w @ cov @ w)

        result = optimize.minimize(
            variance,
            x0,
            method="SLSQP",
            bounds=bounds,
            constraints=constraints,
            options={"maxiter": 1000, "ftol": 1e-12},
        )

    elif target == "max_return":

        def neg_return(w: np.ndarray) -> float:
            return -float(w @ mu)

        result = optimize.minimize(
            neg_return,
            x0,
            method="SLSQP",
            bounds=bounds,
            constraints=constraints,
            options={"maxiter": 1000, "ftol": 1e-12},
        )

    else:
        raise ValueError(f"Unknown target: {target!r}")

    weights = result.x
    # Clip tiny negatives from numerical noise
    weights = np.clip(weights, 0.0, 1.0)
    weights /= weights.sum()
    return weights


# ---------------------------------------------------------------------------
# Risk parity
# ---------------------------------------------------------------------------


def risk_parity_weights(covariance: np.ndarray) -> np.ndarray:
    """Equal risk contribution (risk parity) portfolio.

    Each asset contributes the same amount of risk to the total portfolio
    variance.

    Parameters
    ----------
    covariance:
        ``(N, N)`` covariance matrix.

    Returns
    -------
    np.ndarray:
        ``(N,)`` weight vector summing to 1.
    """
    n = covariance.shape[0]

    def risk_budget_objective(w: np.ndarray) -> float:
        port_vol = np.sqrt(w @ covariance @ w)
        if port_vol < 1e-12:
            return 0.0
        # Marginal risk contribution
        mrc = covariance @ w / port_vol
        # Risk contribution per asset
        rc = w * mrc
        # Target: equal contributions
        target_rc = port_vol / n
        return float(np.sum((rc - target_rc) ** 2))

    constraints = [{"type": "eq", "fun": lambda w: np.sum(w) - 1.0}]
    bounds = [(1e-6, 1.0)] * n
    x0 = np.ones(n) / n

    result = optimize.minimize(
        risk_budget_objective,
        x0,
        method="SLSQP",
        bounds=bounds,
        constraints=constraints,
        options={"maxiter": 1000, "ftol": 1e-14},
    )

    weights = np.clip(result.x, 0.0, 1.0)
    weights /= weights.sum()
    return weights


# ---------------------------------------------------------------------------
# Black-Litterman
# ---------------------------------------------------------------------------


def black_litterman(
    market_weights: np.ndarray,
    covariance: np.ndarray,
    views: list[dict],
    risk_aversion: float = 2.5,
    tau: float = 0.05,
) -> np.ndarray:
    """Black-Litterman model combining market equilibrium with investor views.

    Parameters
    ----------
    market_weights:
        ``(N,)`` market-cap weights.
    covariance:
        ``(N, N)`` covariance matrix.
    views:
        List of view dicts.  Each has:
        - ``assets``: list of asset indices involved
        - ``weights``: list of floats forming the pick vector
        - ``return``: expected return of the view
        - ``confidence``: float in (0, 1]
    risk_aversion:
        Market risk aversion coefficient (lambda).
    tau:
        Scaling factor for the uncertainty of the prior.

    Returns
    -------
    np.ndarray:
        ``(N,)`` posterior optimal weights summing to 1.
    """
    n = len(market_weights)
    sigma = covariance

    # Equilibrium excess returns
    pi = risk_aversion * sigma @ market_weights

    if not views:
        return market_weights.copy()

    k = len(views)
    P = np.zeros((k, n))
    Q = np.zeros(k)
    omega_diag = np.zeros(k)

    for i, v in enumerate(views):
        for asset_idx, w in zip(v["assets"], v["weights"]):
            P[i, asset_idx] = w
        Q[i] = v["return"]
        # Omega: uncertainty of each view
        conf = v.get("confidence", 0.5)
        conf = max(min(conf, 0.999), 0.001)
        omega_diag[i] = (1.0 / conf - 1.0) * (P[i] @ (tau * sigma) @ P[i])

    Omega = np.diag(omega_diag)

    # Posterior parameters
    tau_sigma = tau * sigma
    tau_sigma_inv = np.linalg.inv(tau_sigma)
    P_omega_inv = P.T @ np.linalg.inv(Omega)

    posterior_cov = np.linalg.inv(tau_sigma_inv + P_omega_inv @ P)
    posterior_mu = posterior_cov @ (tau_sigma_inv @ pi + P_omega_inv @ Q)

    # Optimal weights from posterior
    weights = np.linalg.inv(risk_aversion * sigma) @ posterior_mu
    # Normalize to sum to 1 (long-only projection)
    weights = np.clip(weights, 0.0, None)
    total = weights.sum()
    if total > 0:
        weights /= total
    else:
        weights = np.ones(n) / n
    return weights


# ---------------------------------------------------------------------------
# Hierarchical Risk Parity (HRP)
# ---------------------------------------------------------------------------


def _correlation_distance(returns: np.ndarray) -> np.ndarray:
    """Correlation-based distance matrix."""
    corr = np.corrcoef(returns, rowvar=False)
    # Clip for numerical stability
    corr = np.clip(corr, -1.0, 1.0)
    dist = np.sqrt(0.5 * (1 - corr))
    return dist


def _pd_to_list(link: np.ndarray) -> list[int]:
    """Convert linkage matrix to sorted index list via recursive bisection."""
    n = link.shape[0] + 1
    # Build tree
    idx_map: dict[int, list[int]] = {i: [i] for i in range(n)}
    for i, row in enumerate(link):
        left, right = int(row[0]), int(row[1])
        idx_map[n + i] = idx_map[left] + idx_map[right]
    return idx_map[n + len(link) - 1]


def _hrp_alloc(cov: np.ndarray, sorted_ix: list[int]) -> np.ndarray:
    """Recursive bisection allocation (inverse-variance weighted)."""
    n = len(sorted_ix)
    weights = np.ones(n)

    cluster_items = [sorted_ix]

    while len(cluster_items) > 0:
        next_clusters: list[list[int]] = []
        for cluster in cluster_items:
            if len(cluster) <= 1:
                continue
            mid = len(cluster) // 2
            left = cluster[:mid]
            right = cluster[mid:]

            # Variance of each sub-cluster (inverse-variance allocation)
            left_cov = cov[np.ix_(left, left)]
            inv_left = 1.0 / np.diag(left_cov)
            left_var = 1.0 / inv_left.sum()

            right_cov = cov[np.ix_(right, right)]
            inv_right = 1.0 / np.diag(right_cov)
            right_var = 1.0 / inv_right.sum()

            alpha = 1.0 - left_var / (left_var + right_var)

            for ix in left:
                weights[sorted_ix.index(ix)] *= alpha
            for ix in right:
                weights[sorted_ix.index(ix)] *= 1.0 - alpha

            if len(left) > 1:
                next_clusters.append(left)
            if len(right) > 1:
                next_clusters.append(right)

        cluster_items = next_clusters

    weights /= weights.sum()
    return weights


def hierarchical_risk_parity(returns: np.ndarray) -> np.ndarray:
    """Hierarchical Risk Parity (HRP).

    Clusters assets by correlation, then allocates inversely proportional
    to cluster variance using recursive bisection.

    Parameters
    ----------
    returns:
        ``(T, N)`` array of daily asset returns.

    Returns
    -------
    np.ndarray:
        ``(N,)`` weight vector summing to 1.
    """
    n_assets = returns.shape[1]
    if n_assets == 1:
        return np.array([1.0])

    cov = np.cov(returns, rowvar=False) * 252
    dist = _correlation_distance(returns)

    # Convert distance matrix to condensed form for scipy
    np.fill_diagonal(dist, 0.0)
    condensed = squareform(dist, checks=False)

    link = linkage(condensed, method="single")
    sorted_ix = _pd_to_list(link)

    weights = _hrp_alloc(cov, sorted_ix)
    return weights


# ---------------------------------------------------------------------------
# Efficient frontier
# ---------------------------------------------------------------------------


def efficient_frontier(
    returns: np.ndarray,
    n_points: int = 50,
    risk_free: float = 0.02,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Compute the efficient frontier.

    Returns
    -------
    (vols, rets, weights):
        Arrays of portfolio volatilities, returns, and the (n_points, N)
        weight matrix.
    """
    mu, cov = _annualize(returns)
    n_assets = returns.shape[1]

    # Find the range of target returns
    min_ret = float(mu.min())
    max_ret = float(mu.max())
    target_returns = np.linspace(min_ret, max_ret, n_points)

    vols = np.empty(n_points)
    rets = np.empty(n_points)
    all_weights = np.empty((n_points, n_assets))

    for i, t_ret in enumerate(target_returns):
        constraints = [
            {"type": "eq", "fun": lambda w: np.sum(w) - 1.0},
            {"type": "eq", "fun": lambda w, tr=t_ret: w @ mu - tr},
        ]
        bounds = [(0.0, 1.0)] * n_assets
        x0 = np.ones(n_assets) / n_assets

        result = optimize.minimize(
            lambda w: float(w @ cov @ w),
            x0,
            method="SLSQP",
            bounds=bounds,
            constraints=constraints,
            options={"maxiter": 500},
        )

        w = np.clip(result.x, 0.0, 1.0)
        w /= w.sum()
        all_weights[i] = w
        vols[i] = np.sqrt(w @ cov @ w)
        rets[i] = w @ mu

    return vols, rets, all_weights
