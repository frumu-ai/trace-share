#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  echo "example: $0 0.0.2" >&2
  exit 1
fi

VERSION="$1"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  echo "invalid version: $VERSION" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export NEW_VERSION="$VERSION"

perl -0777 -i -pe 's/(\[workspace\.package\][\s\S]*?^\s*version\s*=\s*")[^"]+(")/$1 . $ENV{NEW_VERSION} . $2/gme' Cargo.toml
perl -0777 -i -pe 's/(trace-share-core\s*=\s*\{[^}]*version\s*=\s*")[^"]+(")/$1 . $ENV{NEW_VERSION} . $2/gme' crates/trace-share-cli/Cargo.toml
perl -0777 -i -pe 's/("version"\s*:\s*")[^"]+(")/$1 . $ENV{NEW_VERSION} . $2/e' packages/trace-share/package.json
perl -0777 -i -pe 's/(example:\s*)[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?/$1 . $ENV{NEW_VERSION}/ge' .github/workflows/publish-registries.yml
perl -0777 -i -pe 's/(example:\s*)[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?/$1 . $ENV{NEW_VERSION}/ge' .github/workflows/publish-snapshot.yml

echo "updated version to $VERSION in:"
echo "- Cargo.toml (workspace.package.version)"
echo "- crates/trace-share-cli/Cargo.toml (trace-share-core dependency version)"
echo "- packages/trace-share/package.json (npm package version)"
echo "- .github/workflows/publish-registries.yml (input example)"
echo "- .github/workflows/publish-snapshot.yml (input example)"
