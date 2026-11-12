# Contributing

Build Engine is fair-source software, released under the Build Engine
FSL-1.1-MIT.

Issues, bug reports, reproducible test cases, and documentation corrections are
welcome. Code contributions are not accepted unless there is a prior written
agreement that assigns or licenses the contribution to Zain Dana Harper on terms
compatible with this repository.

## What Helps

- A minimal reproduction.
- The exact command you ran.
- The Python version and operating system.
- Whether the run used paper mode or live mode.
- Sanitized logs with secrets removed.

## What Not To Submit

- API keys, broker tokens, passwords, private account data, or `.env` files.
- Live trading screenshots containing account identifiers.
- Proprietary third-party source code.
- Legal or financial claims about model performance.

## Live Broker Reports

Reports involving live broker behavior must be reproducible without exposing
credentials. Use placeholders for environment variables:

```text
APCA_API_KEY_ID=<redacted>
APCA_API_SECRET_KEY=<redacted>
```

## License Boundary

Opening an issue or pull request does not grant rights to use this software
beyond the repository license. Unsolicited code may be closed without merge if
the licensing path is unclear.
