#!/usr/bin/env python
"""
Block Secrets Hook — PreToolUse
Prevents Claude from reading or editing sensitive files.
Exit code 2 = block operation and tell Claude why.

Based on Claude Code Mastery Guides V1-V5 by TheDecipherist
"""
import json
import sys
from pathlib import Path

# Files that should NEVER be read or edited by Claude (basename match — anchored)
SENSITIVE_FILENAMES = {
    '.env',
    '.env.local',
    '.env.production',
    '.env.staging',
    '.env.development',
    'secrets.json',
    'secrets.yaml',
    'id_rsa',
    'id_ed25519',
    'id_ecdsa',
    'id_dsa',
    '.npmrc',        # may contain auth tokens
    '.pypirc',       # PyPI auth tokens
    'credentials.json',
    'service-account.json',
}

# Path components (between separators) that mark sensitive directories.
# Anchored to path parts — NOT substring — so workspace Python modules whose
# names happen to contain "private_key" / "secret_key" / "ssh" are not blocked,
# while real OS credential dirs (~/.ssh, ~/.aws, ~/.gnupg, ~/.docker) still are.
SENSITIVE_PATH_COMPONENTS = {
    '.ssh',
    '.aws',
    '.gnupg',
    '.docker',
}

try:
    data = json.load(sys.stdin)
    tool_name = data.get('tool_name', '')
    file_path = data.get('tool_input', {}).get('file_path', '')

    if not file_path:
        sys.exit(0)

    path = Path(file_path)

    # Allow Write to .env files (needed for /new-project scaffolding).
    # Only block Read/Edit which could leak existing secrets.
    if tool_name == 'Write' and path.name.startswith('.env'):
        sys.exit(0)

    # Check exact filename matches
    if path.name in SENSITIVE_FILENAMES:
        print(f"BLOCKED: Access to '{file_path}' denied. This is a sensitive file.", file=sys.stderr)
        sys.exit(2)

    # Check path component matches (anchored, not substring — see SENSITIVE_PATH_COMPONENTS comment).
    for component in SENSITIVE_PATH_COMPONENTS:
        if component in path.parts:
            print(f"BLOCKED: Access to '{file_path}' denied. Path contains sensitive component '{component}'.", file=sys.stderr)
            sys.exit(2)

    sys.exit(0)

except Exception as e:
    print(f"Hook error: {e}", file=sys.stderr)
    sys.exit(1)
