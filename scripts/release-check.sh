#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
	cat <<'EOF'
Usage: scripts/release-check.sh [fast|full]

Modes:
  fast  Run clippy, tests, and CLI help. Use during local iteration.
  full  Run the release smoke script plus release build, man page, install dry-run,
        and Cargo package file listing. This is the default release gate.
EOF
}

MODE="${1:-full}"

case "$MODE" in
	fast | full)
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		usage >&2
		echo "Unknown release-check mode: $MODE" >&2
		exit 2
		;;
esac

run_fast() {
	rtk cargo clippy -- -D warnings
	rtk cargo test
	rtk cargo run -- --help >/dev/null
}

run_full() {
	rtk scripts/smoke.sh
	rtk cargo build --release
	rtk target/release/fcs --version
	rtk target/release/fcs --help
	rtk target/release/fcs man --stdout >/dev/null
	rtk scripts/install-local.sh --dry-run --prefix /tmp/fcs-release-install
	rtk cargo package --list --allow-dirty
}

if [[ "$MODE" == "fast" ]]; then
	run_fast
else
	run_full
fi

echo "fcs release check ($MODE) passed"
