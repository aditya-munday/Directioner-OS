#!/usr/bin/env bash
set -euo pipefail

# Rule B2 Restricted Files Guard
# Fails if any restricted file was modified without the explicit owner authorization marker in git commit.

AUTH_MARKER="OWNER-AUTH: AUTHORIZE-RESTRICTED-FILE-EDIT"

RESTRICTED_FILES=(
  "ai-engineer/control-plane/src/policy.rs"
  "ai-engineer/control-plane/src/reviewer.rs"
  "ai-engineer/control-plane/src/self_healing.rs"
  "ai-engineer/control-plane/src/incident_response.rs"
  "ai-engineer/control-plane/src/conversational.rs"
)

# Determine the base commit to check against (default: HEAD~1 or origin/main)
BASE_REF="${1:-HEAD~1}"

echo "Checking commit range ${BASE_REF}..HEAD for restricted file modifications (Rule B2)..."

COMMIT_MSG=$(git log -1 --pretty=%B)
if echo "${COMMIT_MSG}" | grep -q "${AUTH_MARKER}"; then
  echo "Owner authorization marker detected in commit message. Check passed."
  exit 0
fi

CHANGED_FILES=$( { git diff --name-only "${BASE_REF}" HEAD 2>/dev/null || true; git diff --name-only HEAD 2>/dev/null || true; } | sort -u )

VIOLATIONS=()
for rf in "${RESTRICTED_FILES[@]}"; do
  if echo "${CHANGED_FILES}" | grep -Fxq "${rf}"; then
    VIOLATIONS+=("${rf}")
  fi
done

# Also check if CapabilityKind enum in capability-layer was modified
if echo "${CHANGED_FILES}" | grep -Fxq "ai-engineer/capability-layer/src/lib.rs"; then
  if git diff "${BASE_REF}" HEAD -- "ai-engineer/capability-layer/src/lib.rs" 2>/dev/null | grep -E '^[+-].*enum CapabilityKind' >/dev/null; then
    VIOLATIONS+=("ai-engineer/capability-layer/src/lib.rs (CapabilityKind enum)")
  fi
fi

if [ "${#VIOLATIONS[@]}" -gt 0 ]; then
  echo "================================================================================"
  echo "RULE B2 VIOLATION: Restricted files modified without authorization!"
  echo "The following restricted files were modified:"
  for v in "${VIOLATIONS[@]}"; do
    echo "  - ${v}"
  done
  echo "To authorize this change, the commit message must include the exact marker:"
  echo "  ${AUTH_MARKER}"
  echo "================================================================================"
  exit 1
fi

echo "No restricted file violations found. Check passed."
exit 0
