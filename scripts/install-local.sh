#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
DRY_RUN=0

while [[ $# -gt 0 ]]; do
	case "$1" in
		--prefix)
			PREFIX="$2"
			shift 2
			;;
		--dry-run)
			DRY_RUN=1
			shift
			;;
		-h|--help)
			echo "usage: scripts/install-local.sh [--prefix DIR] [--dry-run]"
			exit 0
			;;
		*)
			echo "unknown argument: $1" >&2
			exit 2
			;;
	esac
done

BIN_DIR="$PREFIX/bin"
MAN_DIR="$PREFIX/share/man/man1"

run() {
	if [[ "$DRY_RUN" -eq 1 ]]; then
		printf '+'
		printf ' %q' "$@"
		printf '\n'
	else
		rtk "$@"
	fi
}

run cargo build --release
run mkdir -p "$BIN_DIR" "$MAN_DIR"
run cp target/release/fcs "$BIN_DIR/fcs"
run target/release/fcs dev man --out-dir "$MAN_DIR"

echo "Installed fcs to $BIN_DIR/fcs"
