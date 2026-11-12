#!/usr/bin/env bash
# Lint on Save Hook — PostToolUse (Python projects)

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | python -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('file_path',''))" 2>/dev/null)

if [ -z "$FILE_PATH" ]; then exit 0; fi

EXTENSION="${FILE_PATH##*.}"

case "$EXTENSION" in
    py)
        if command -v ruff &> /dev/null; then
            ruff check "$FILE_PATH" 2>&1 | head -20
        fi
        ;;
    yml|yaml)
        if command -v yamllint &> /dev/null; then
            yamllint "$FILE_PATH" 2>&1 | head -10
        fi
        ;;
esac

exit 0
