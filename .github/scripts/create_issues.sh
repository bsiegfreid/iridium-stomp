#!/usr/bin/env bash
set -euo pipefail

# create_issues.sh
# Reads markdown files from .github/issues/*.md and creates GitHub issues.
# Prefer `gh` if present; otherwise use curl and GITHUB_TOKEN.

REPO="${GITHUB_REPOSITORY:-bsiegfreid/iridium-stomp}"
ISSUE_DIR="$(cd "$(dirname "$0")/../issues" && pwd)"

shopt -s nullglob
files=("$ISSUE_DIR"/*.md)
if [ ${#files[@]} -eq 0 ]; then
  echo "No issue files found in $ISSUE_DIR"
  exit 0
fi

for f in "${files[@]}"; do
  echo "\n---\nProcessing $f"
  title=$(grep -m1 '^Title:' "$f" | sed 's/^Title:[[:space:]]*//')
  if [ -z "$title" ]; then
    echo "  Skipping $f: no Title: header found"
    continue
  fi

  # Body: everything after a line that starts with `Body:`
  body=$(awk 'BEGIN{p=0} /^Body:/{p=1; next} {if(p) print}' "$f")

  if command -v gh >/dev/null 2>&1; then
    echo "  Creating issue via gh: $title"
    gh issue create --title "$title" --body "$body" --repo "$REPO"
  else
    if [ -z "${GITHUB_TOKEN:-}" ]; then
      echo "  gh not available and GITHUB_TOKEN is not set; skipping $title"
      continue
    fi
    api_url="https://api.github.com/repos/$REPO/issues"
    # Use jq if available for safe JSON, otherwise simple escaping
    if command -v jq >/dev/null 2>&1; then
      json=$(jq -n --arg t "$title" --arg b "$body" '{title:$t, body:$b}')
    else
      # Minimal JSON escaping (best-effort)
      esc_body=$(printf '%s' "$body" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
      esc_title=$(printf '%s' "$title" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
      # python prints quoted strings; we need raw values
      json=$(printf '{"title":%s, "body":%s}' "$esc_title" "$esc_body")
    fi

    echo "  Creating issue via GitHub API: $title"
    resp=$(curl -sS -H "Authorization: token $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" "$api_url" -d "$json")
    # Print created issue URL if present
    echo "$resp" | (command -v jq >/dev/null 2>&1 && jq -r '.html_url // .url' || sed -n 's/.*"html_url": "\([^"]*\)".*/\1/p')
  fi
done

echo "\nAll done. Check the repository issues: https://github.com/$REPO/issues"
