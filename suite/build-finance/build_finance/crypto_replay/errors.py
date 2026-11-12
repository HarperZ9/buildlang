"""Errors raised by crypto replay contract helpers."""


class CanonicalJSONError(ValueError):
    """Raised when JSON bytes do not satisfy the canonical contract."""


class DuplicateKeyError(CanonicalJSONError):
    """Raised when a JSON object contains a duplicate key."""

    def __init__(self, key: str) -> None:
        super().__init__(f"duplicate object key: {key!r}")
        self.key = key
