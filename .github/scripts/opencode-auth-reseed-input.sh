#!/bin/bash
set -euo pipefail

DEFAULT_AUTH_FILE="$HOME/.local/share/opencode/auth.json"
AUTH_FILE="$DEFAULT_AUTH_FILE"
COPY_TO_CLIPBOARD=false
SET_SECRET=false
REPO=""
AUTH_FILE_SET=false

detect_repo() {
    local remote
    local parsed

    if ! remote="$(git remote get-url origin 2>/dev/null)"; then
        return 1
    fi

    parsed="$(python3 - "$remote" << 'PY'
import re
import sys

remote = sys.argv[1]
patterns = [
    r"^git@github\.com:([^/]+)/([^/]+?)(?:\.git)?$",
    r"^https://github\.com/([^/]+)/([^/]+?)(?:\.git)?$",
    r"^ssh://git@github\.com/([^/]+)/([^/]+?)(?:\.git)?$",
]

for pattern in patterns:
    match = re.match(pattern, remote)
    if match:
        print(f"{match.group(1)}/{match.group(2)}")
        raise SystemExit(0)

raise SystemExit(1)
PY
    )" || return 1

    if [ -z "$parsed" ]; then
        return 1
    fi

    printf '%s\n' "$parsed"
}

usage() {
    cat << 'EOF'
Usage: opencode-auth-reseed-input.sh [--copy] [--set-secret] [--repo owner/name] [auth_file]

Generate the single-line base64 value for OpenCode auth reseed workflow input.

Options:
  --copy             Copy the generated value to clipboard (macOS pbcopy)
  --set-secret       Update OPENCODE_AUTH_JSON_B64 using gh CLI
  --repo owner/name  Repository for --set-secret (defaults to origin remote)
  --help             Show this help message

Arguments:
  auth_file          Optional path to auth.json (default: ~/.local/share/opencode/auth.json)

Examples:
  opencode-auth-reseed-input.sh
  opencode-auth-reseed-input.sh --copy
  opencode-auth-reseed-input.sh --set-secret
  opencode-auth-reseed-input.sh --set-secret --repo MoosicBox/MoosicBox
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --copy)
            COPY_TO_CLIPBOARD=true
            shift
            ;;
        --set-secret)
            SET_SECRET=true
            shift
            ;;
        --repo)
            if [ "$#" -lt 2 ]; then
                echo "Error: --repo requires a value like owner/name" >&2
                usage >&2
                exit 1
            fi
            REPO="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --*)
            echo "Error: Unknown option '$1'" >&2
            usage >&2
            exit 1
            ;;
        *)
            if [ "$AUTH_FILE_SET" = true ]; then
                echo "Error: Multiple auth file paths provided" >&2
                usage >&2
                exit 1
            fi
            AUTH_FILE="$1"
            AUTH_FILE_SET=true
            shift
            ;;
    esac
done

if [ ! -f "$AUTH_FILE" ]; then
    echo "Error: Auth file not found: $AUTH_FILE" >&2
    exit 1
fi

python3 - "$AUTH_FILE" << 'PY'
import json
import pathlib
import sys

auth = json.loads(pathlib.Path(sys.argv[1]).read_text())
openai = auth.get("openai")
if openai is None and isinstance(auth.get("providers"), dict):
    openai = auth["providers"].get("openai")

if not isinstance(openai, dict):
    raise SystemExit("Error: Missing openai provider in auth payload")
if not isinstance(openai.get("access"), str) or not openai.get("access"):
    raise SystemExit("Error: Missing openai.access in auth payload")
if not isinstance(openai.get("refresh"), str) or not openai.get("refresh"):
    raise SystemExit("Error: Missing openai.refresh in auth payload")
PY

VALUE="$(base64 < "$AUTH_FILE" | tr -d '\n')"

if [ "$SET_SECRET" = true ]; then
    if ! command -v gh >/dev/null 2>&1; then
        echo "Error: --set-secret requested but gh is not installed" >&2
        exit 1
    fi

    if ! gh auth status >/dev/null 2>&1; then
        echo "Error: gh is not authenticated. Run 'gh auth login' first." >&2
        exit 1
    fi

    if [ -z "$REPO" ]; then
        if REPO="$(detect_repo)"; then
            :
        else
            echo "Error: Could not infer GitHub repository from origin remote; pass --repo owner/name" >&2
            exit 1
        fi
    fi

    printf '%s' "$VALUE" | gh secret set OPENCODE_AUTH_JSON_B64 --repo "$REPO"
    echo "Updated OPENCODE_AUTH_JSON_B64 in $REPO."
fi

if [ "$COPY_TO_CLIPBOARD" = true ]; then
    if command -v pbcopy >/dev/null 2>&1; then
        printf '%s' "$VALUE" | pbcopy
        echo "Copied reseed value to clipboard."
    else
        echo "Error: --copy requested but pbcopy is not available" >&2
        exit 1
    fi
fi

printf '%s\n' "$VALUE"
