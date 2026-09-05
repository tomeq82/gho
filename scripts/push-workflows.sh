#!/usr/bin/env bash
# Push the GitHub Actions workflow files via the GitHub Contents API.
#
# The OAuth app token used by `gh auth` cannot write to .github/workflows/
# (GitHub security feature). This script uses a personal access token (PAT)
# with `workflow` scope to push the workflow files. Run this once with your
# PAT exported as $GH_TOKEN.
#
# Usage:
#   GH_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx \
#     bash scripts/push-workflows.sh
set -euo pipefail

if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "Error: GH_TOKEN environment variable is not set" >&2
  echo "Create a PAT at https://github.com/settings/tokens/new with 'workflow' scope" >&2
  exit 1
fi

REPO="tomeq82/gho"
FILES=(
  ".github/workflows/ci.yml"
  ".github/workflows/release.yml"
  ".github/workflows/fuzz.yml"
  ".github/workflows/docker.yml"
)

for f in "${FILES[@]}"; do
  echo "Pushing $f..."
  CONTENT=$(base64 -w 0 "$f")
  # Get the current SHA for existing files (needed for updates)
  SHA=$(curl -s -H "Authorization: Bearer ${GH_TOKEN}" \
    "https://api.github.com/repos/${REPO}/contents/${f}" 2>/dev/null \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('sha',''))" 2>/dev/null || echo "")
  if [[ -n "${SHA}" ]]; then
    PAYLOAD=$(jq -n --arg msg "Update ${f}" --arg content "${CONTENT}" --arg sha "${SHA}" '{message: $msg, content: $content, sha: $sha}')
  else
    PAYLOAD=$(jq -n --arg msg "Add ${f}" --arg content "${CONTENT}" '{message: $msg, content: $content}')
  fi
  HTTP_CODE=$(curl -s -o /tmp/push-resp.json -w "%{http_code}" \
    -X PUT "https://api.github.com/repos/${REPO}/contents/${f}" \
    -H "Authorization: Bearer ${GH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "${PAYLOAD}")
  if [[ "${HTTP_CODE}" =~ ^2 ]]; then
    echo "  OK"
  else
    echo "  FAILED (HTTP ${HTTP_CODE}):"
    cat /tmp/push-resp.json
    echo
    exit 1
  fi
done

echo
echo "All workflow files pushed. Visit https://github.com/${REPO}/actions to enable them."
