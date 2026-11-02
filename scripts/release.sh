#!/usr/bin/env bash
# Make a new release: bump the version, commit, tag and push. Pushing the tag
# makes GitHub build the DMG (.github/workflows/release.yml).
#   scripts/release.sh          1.0.3 → 1.0.4
#   scripts/release.sh minor    1.0.3 → 1.1.0
#   scripts/release.sh major    1.0.3 → 2.0.0
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || source "$HOME/.cargo/env"

part=${1:-patch}
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
IFS=. read -r major minor patch <<<"$current"
case "$part" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
    *) echo "Usage: $0 [patch|minor|major]" >&2; exit 1 ;;
esac
next="$major.$minor.$patch"
tag="v$next"

branch=$(git branch --show-current)
if [[ "$branch" != "main" ]]; then
    echo "You are on branch '$branch', not 'main'." >&2
    exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "Tag $tag already exists." >&2
    exit 1
fi
git fetch -q origin
if [[ -n "$(git rev-list HEAD..origin/main)" ]]; then
    echo "origin/main has commits you don't have yet; run 'git pull' first." >&2
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo "These changes are not committed yet:"
    git status --short
    read -r -p "Include them in the $tag commit? [y/N] " answer
    [[ "$answer" == [yY] ]] || { echo "Stopped; nothing changed."; exit 1; }
fi

echo "== Testing"
cargo test --quiet
cargo clippy --quiet --all-targets -- -D warnings
scripts/check-test-parity.sh

echo "== $current → $next"
# Only the package's own version starts a line; dependencies use inline tables.
sed -i.bak "s/^version = \"$current\"/version = \"$next\"/" Cargo.toml
rm -f Cargo.toml.bak
cargo check --quiet # updates the version in Cargo.lock
if command -v cargo-about >/dev/null; then
    scripts/licenses.sh
else
    echo "(cargo-about not installed; third-party licenses not refreshed)"
fi

git add -A
git commit -q -m "VER: Priogator $next"
git tag "$tag"
echo "== Committed and tagged $tag"
read -r -p "Push to GitHub and start the build? [Y/n] " answer
if [[ "$answer" == [nN] ]]; then
    echo "Not pushed. Later: git push && git push origin $tag"
    exit 0
fi
git push -q
git push -q origin "$tag"
echo "== Pushed. Follow the build with: gh run watch"
echo "   The DMG will be at: https://github.com/zoglauer/priogator/releases/tag/$tag"
