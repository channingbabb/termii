#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CARGO_TOML="${REPO_ROOT}/Cargo.toml"

# read the current version
CURRENT=$(grep '^version' "${CARGO_TOML}" | head -1 | sed 's/version = "//;s/"//')

usage() {
    echo "Usage: $0 <patch|minor|major|x.y.z>"
    echo ""
    echo "  patch | Increment patch  (e.g. 0.1.0 -> 0.1.1)"
    echo "  minor | Increment minor  (e.g. 0.1.0 -> 0.2.0)"
    echo "  major | Increment major  (e.g. 0.1.0 -> 1.0.0)"
    echo "  x.y.z | Set explicit version"
    echo ""
    echo "Current version: ${CURRENT}"
    exit 1
}

[ $# -eq 0 ] && usage

# calc the new version
IFS='.' read -r MAJOR MINOR PATCH <<< "${CURRENT}"

case "$1" in
    patch) PATCH=$((PATCH + 1)); NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}" ;;
    minor) MINOR=$((MINOR + 1)); PATCH=0; NEW_VERSION="${MAJOR}.${MINOR}.0" ;;
    major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0; NEW_VERSION="${MAJOR}.0.0" ;;
    *)
        if ! echo "$1" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
            echo "Error: '$1' is not a valid version. Use x.y.z or patch/minor/major."
            exit 1
        fi
        NEW_VERSION="$1"
        ;;
esac

echo "Bumping version: ${CURRENT} -> ${NEW_VERSION}"

#cases for when there are errors
if ! git -C "${REPO_ROOT}" diff --quiet || ! git -C "${REPO_ROOT}" diff --cached --quiet; then
    echo "Error: Working tree has uncommitted changes. Commit or stash them first."
    exit 1
fi

if git -C "${REPO_ROOT}" tag | grep -qx "v${NEW_VERSION}"; then
    echo "Error: Tag v${NEW_VERSION} already exists."
    exit 1
fi

# update the version in cargo.toml
perl -i -pe "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" "${CARGO_TOML}"

# workflow to bump version
BRANCH="$(git -C "${REPO_ROOT}" rev-parse --abbrev-ref HEAD)"

git -C "${REPO_ROOT}" add "${CARGO_TOML}"
git -C "${REPO_ROOT}" commit -m "chore: bump version to ${NEW_VERSION}"
git -C "${REPO_ROOT}" tag "v${NEW_VERSION}"
git -C "${REPO_ROOT}" push origin "${BRANCH}"
git -C "${REPO_ROOT}" push origin "v${NEW_VERSION}"

echo ""
echo "Done! v${NEW_VERSION} is tagged and pushed."
echo "GitHub Actions will now build and publish the installers."
