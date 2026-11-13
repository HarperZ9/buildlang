"""Run one import or pytest invocation with process-local network denial."""

from __future__ import annotations

import _socket
import importlib
import json
import socket
import sys
from collections.abc import Callable, Sequence
from contextlib import contextmanager
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


class NetworkDenied(RuntimeError):
    """A denied socket or name-resolution primitive was called."""


_TARGETS = (
    (socket, "create_connection"),
    (socket, "create_server"),
    (socket, "fromfd"),
    (socket, "fromshare"),
    (socket, "getaddrinfo"),
    (socket, "gethostbyaddr"),
    (socket, "gethostbyname"),
    (socket, "gethostbyname_ex"),
    (socket, "getnameinfo"),
    (socket.socket, "accept"),
    (socket.socket, "bind"),
    (socket.socket, "connect"),
    (socket.socket, "connect_ex"),
    (socket.socket, "listen"),
    (socket.socket, "recv"),
    (socket.socket, "recv_into"),
    (socket.socket, "recvfrom"),
    (socket.socket, "recvfrom_into"),
    (socket.socket, "recvmsg"),
    (socket.socket, "recvmsg_into"),
    (socket.socket, "send"),
    (socket.socket, "sendall"),
    (socket.socket, "sendmsg"),
    (socket.socket, "sendto"),
    (socket, "socketpair"),
    (socket, "SocketType"),
    (socket, "socket"),
    (_socket, "socket"),
)


@contextmanager
def deny_network(events: list[str]):
    """Replace socket/DNS entry points and restore them after the action."""

    originals: list[tuple[Any, str, Any]] = []

    def denied(name: str) -> Callable[..., Any]:
        def reject(*_args: object, **_kwargs: object) -> Any:
            events.append(name)
            raise NetworkDenied(f"network primitive denied: {name}")

        return reject

    def denied_socket_type(original: type[Any], name: str) -> type[Any]:
        class DeniedSocket(original):
            def __new__(cls, *_args: object, **_kwargs: object) -> Any:
                events.append(name)
                raise NetworkDenied(f"network primitive denied: {name}")

        DeniedSocket.__name__ = original.__name__
        DeniedSocket.__qualname__ = original.__qualname__
        return DeniedSocket

    try:
        for owner, name in _TARGETS:
            if not hasattr(owner, name):
                continue
            original = getattr(owner, name)
            target_name = f"{owner.__name__}.{name}"
            originals.append((owner, name, original))
            replacement = denied_socket_type(original, target_name) if isinstance(original, type) else denied(target_name)
            setattr(owner, name, replacement)
        yield
    finally:
        for owner, name, original in reversed(originals):
            setattr(owner, name, original)


def _prove_shim(events: list[str]) -> None:
    for operation in (
        lambda: socket.getaddrinfo("network-denied.invalid", 443),
        lambda: socket.socket().connect(("127.0.0.1", 9)),
    ):
        try:
            operation()
        except NetworkDenied:
            continue
        raise AssertionError("network denial self-test did not intercept a primitive")
    if len(events) != 2:
        raise AssertionError(f"network denial self-test expected 2 interceptions, got {events!r}")
    events.clear()


def _usage() -> str:
    return "usage: run_network_denied.py (--import MODULE | --pytest PYTEST_ARG [...])"


def main(argv: Sequence[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if not args or args[0] not in {"--import", "--pytest"}:
        print(_usage(), file=sys.stderr)
        return 2
    mode = args.pop(0)
    if not args or (mode == "--import" and len(args) != 1):
        print(_usage(), file=sys.stderr)
        return 2

    events: list[str] = []
    exit_code = 0
    error: str | None = None
    error_type: str | None = None
    self_test = "PENDING"
    root_inserted = False
    try:
        with deny_network(events):
            _prove_shim(events)
            self_test = "PASS"
            sys.path.insert(0, str(ROOT))
            root_inserted = True
            if mode == "--import":
                importlib.import_module(args[0])
            else:
                import pytest

                exit_code = int(pytest.main(args))
    except Exception as caught:
        exit_code = 1
        error = str(caught)
        error_type = type(caught).__name__
    finally:
        if root_inserted and sys.path[0] == str(ROOT):
            sys.path.pop(0)

    summary = {
        "action": "import" if mode == "--import" else "pytest",
        "blocked_calls": events,
        "error": error,
        "error_type": error_type,
        "exit_code": exit_code,
        "network": "DENIED",
        "self_test": self_test,
    }
    print(json.dumps(summary, separators=(",", ":"), sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
