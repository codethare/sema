#!/usr/bin/env bash
# ============================================================================
# sema — Standardized Release Script
# ============================================================================
# Usage:
#   ./scripts/release.sh           # patch bump (0.0.X)
#   ./scripts/release.sh minor     # minor bump (0.X.0)
#   ./scripts/release.sh major     # major bump (X.0.0)
#   ./scripts/release.sh 0.1.0     # explicit version
#
# Requires: clean working directory, on master branch, git + cargo.
# ============================================================================
set -euo pipefail

REPO="codethare/sema"
BRANCH="master"

# ---------------------------------------------------------------------------
# Color helpers
# ---------------------------------------------------------------------------
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'
NC='\033[0m'
die()  { echo -e "${RED}error:${NC} $*" >&2; exit 1; }
info() { echo -e "${CYAN}::${NC} $*"; }
ok()   { echo -e "${GREEN}ok${NC} $*"; }
warn() { echo -e "${YELLOW}warning:${NC} $*"; }

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------
command -v git  >/dev/null || die "git is required"
command -v cargo >/dev/null || die "cargo is required"

# Must be on the release branch
CUR_BRANCH=$(git rev-parse --abbrev-ref HEAD)
[[ "$CUR_BRANCH" == "$BRANCH" ]] || \
  die "must be on '$BRANCH' (currently on '$CUR_BRANCH')"

# Working directory must be clean
[[ -z "$(git status --porcelain)" ]] || \
  die "working directory has uncommitted changes; commit or stash first"

# Up to date with remote?
UPSTREAM=$(git rev-parse "@{upstream}" 2>/dev/null || true)
if [[ -n "$UPSTREAM" ]]; then
  BEHIND=$(git rev-list --count "HEAD..@{upstream}" 2>/dev/null || echo 0)
  if [[ "$BEHIND" -gt 0 ]]; then
    die "local branch is behind origin/$BRANCH by $BEHIND commits; pull first"
  fi
fi

# ---------------------------------------------------------------------------
# Determine new version
# ---------------------------------------------------------------------------
CURRENT=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
info "current version: $CURRENT"

bump_semver() {
  local ver="$1" part="$2"
  local major minor patch
  IFS='.' read -r major minor patch <<< "$ver"
  case "$part" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) die "unknown bump type: $part (use patch|minor|major)" ;;
  esac
  echo "$major.$minor.$patch"
}

if [[ $# -eq 0 ]]; then
  NEW=$(bump_semver "$CURRENT" patch)
elif [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  NEW="$1"
elif [[ "$1" =~ ^(patch|minor|major)$ ]]; then
  NEW=$(bump_semver "$CURRENT" "$1")
else
  die "usage: $0 [patch|minor|major|X.Y.Z]"
fi

# Ensure version actually increased
LOWER=$(printf "%s\n%s" "$CURRENT" "$NEW" | sort -V | head -1)
[[ "$LOWER" != "$NEW" ]] && [[ "$CURRENT" != "$NEW" ]] || \
  die "new version ($NEW) must be greater than current ($CURRENT)"

info "new version:  $NEW"

sed -i "s/^version = \"$CURRENT\"/version = \"$NEW\"/" Cargo.toml
ok "Cargo.toml updated"

cargo check --quiet 2>/dev/null
ok "Cargo.lock updated"

TODAY=$(date +%Y-%m-%d)
if grep -q "^# Changelog" CHANGELOG.md; then
  sed -i "3i\\\n## [$NEW] — $TODAY\n\n### Added\n\n-\n\n### Changed\n\n-\n\n### Fixed\n\n-\n" CHANGELOG.md
  warn "CHANGELOG.md — a draft entry was inserted; please edit it before committing"
else
  warn "CHANGELOG.md missing '# Changelog' header; skipping auto-insert"
fi

# ---------------------------------------------------------------------------
# Show summary & confirm
# ---------------------------------------------------------------------------
echo ""
info "Release summary:"
echo "  Current:  $CURRENT"
echo "  New:      $NEW"
echo "  Branch:   $CUR_BRANCH"
echo "  Remote:   origin/$BRANCH"
echo "  Tag:      v$NEW"
echo ""

# ---------------------------------------------------------------------------
# Commit
# ---------------------------------------------------------------------------
info "staging changes..."
git add Cargo.toml Cargo.lock CHANGELOG.md
git diff --cached --stat

echo ""
read -r -p "Proceed with release v$NEW? [y/N] " CONFIRM
[[ "$CONFIRM" =~ ^[yY] ]] || die "aborted by user"

info "committing..."
git commit \
  -m "v$NEW" \
  -m "Ultraworked with [Sisyphus](https://github.com/code-yeongyu/oh-my-openagent)" \
  -m "Co-authored-by: Sisyphus <clio-agent@sisyphuslabs.ai>"
ok "commit created"

# ---------------------------------------------------------------------------
# Tag
# ---------------------------------------------------------------------------
info "tagging v$NEW..."
git tag -a "v$NEW" -m "v$NEW"
ok "tagged v$NEW"

# ---------------------------------------------------------------------------
# Verify
# ---------------------------------------------------------------------------
info "verifying..."
git log --oneline -3
echo ""

# ---------------------------------------------------------------------------
# Push
# ---------------------------------------------------------------------------
echo ""
read -r -p "Push to origin/$BRANCH and tags? [y/N] " PUSH_CONFIRM
if [[ "$PUSH_CONFIRM" =~ ^[yY] ]]; then
  git push origin "$BRANCH"
  git push origin "v$NEW"
  ok "pushed to origin/$BRANCH and v$NEW"
  echo ""
  echo "  https://github.com/$REPO/releases/tag/v$NEW"
else
  warn "not pushed. push manually when ready:"
  echo "  git push origin $BRANCH"
  echo "  git push origin v$NEW"
fi

echo ""
ok "release v$NEW complete!"
