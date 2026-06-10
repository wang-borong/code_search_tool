#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

rtk scripts/smoke.sh
rtk cargo build --release
rtk target/release/fcs --version
rtk target/release/fcs --help
rtk target/release/fcs man --stdout >/dev/null
rtk scripts/install-local.sh --dry-run --prefix /tmp/fcs-release-install
rtk cargo package --list --allow-dirty

echo "fcs release check passed"
