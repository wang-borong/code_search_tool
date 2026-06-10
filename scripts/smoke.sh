#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export XDG_CACHE_HOME="/tmp/fcs-smoke-cache-${RANDOM}-$$"
PROFILE_NAME="fcs-smoke-$$"
DAP_PROFILE_NAME="fcs-dap-smoke-$$"
TRACE_SESSION_NAME="fcs-trace-smoke-$$"
SMOKE_ROOT="/tmp/fcs-smoke-workspace-${RANDOM}-$$"

rtk mkdir -p "$SMOKE_ROOT"
rtk tee "$SMOKE_ROOT/compile_flags.txt" >/dev/null <<'EOF'
-std=c11
EOF
rtk tee "$SMOKE_ROOT/main.c" >/dev/null <<'EOF'
#include <stdio.h>

static void say_hello(void)
{
	printf("hello\n");
}

int main(void)
{
	say_hello();
	return 0;
}
EOF
rtk tee "$SMOKE_ROOT/.fcs.toml" >/dev/null <<'EOF'
[[actions]]
name = "smoke"
description = "print expanded action context"
command = "echo"
args = ["{symbol}", "{file}:{line}", "{workspace}"]
cwd = "{workspace}"
EOF

rtk cargo test
rtk cargo clippy -- -D warnings
rtk cargo build

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs complete bash >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs complete zsh >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs tui --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace status
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace advise
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace detect
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace advise "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace detect "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index build "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index stats "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index compact "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index prewarm "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index refresh "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index list "$SMOKE_ROOT" --kind symbols --limit 5
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index query main "$SMOKE_ROOT" --kind symbols --limit 5 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index repair "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index bench "$SMOKE_ROOT" --limit 5 --query main
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs files --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs symbol --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs type-def --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs doc-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs outgoing --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs hover --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp health "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph imports "$SMOKE_ROOT" --limit 5 --format text
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph modules "$SMOKE_ROOT" --limit 5 --format dot --depth 2
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph calls "$SMOKE_ROOT" --limit 5 --format json --fanout 4
if rtk clangd --version >/dev/null 2>&1; then
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format text
fi

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace export --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:8:5" --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace note latest "smoke note"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace status latest open
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace priority latest high
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:4:2" --session "${TRACE_SESSION_NAME}-next" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace list --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace report "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace timeline "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace replay "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace structured "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace diff "$TRACE_SESSION_NAME" "${TRACE_SESSION_NAME}-next" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace archive "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions --archived
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace unarchive "$TRACE_SESSION_NAME"

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions list "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions templates
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions init make-test --directory "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions run smoke --directory "$SMOKE_ROOT" --file "$SMOKE_ROOT/main.c" --line 8 --symbol main --dry-run -- --extra
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin list "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin show builtin-dev --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin templates "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin commands "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin init builtin-dev:rust-debug --directory "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin run builtin-dev:cargo-check --directory "$SMOKE_ROOT" --dry-run -- --locked

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug command target/debug/fcs -b src/main.rs:1 --cwd . --env FCS_SMOKE=1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug save-profile "$PROFILE_NAME" target/debug/fcs -b src/main.rs:1 --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug from-trace "$TRACE_SESSION_NAME" target/debug/fcs --name "${PROFILE_NAME}-trace" --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug profiles "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug disable-breakpoint "$PROFILE_NAME" 1 --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug enable-breakpoint "$PROFILE_NAME" 1 --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug run-profile "$PROFILE_NAME" --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug delete-profile "$PROFILE_NAME" --directory "$SMOKE_ROOT"

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap launch target/debug/fcs -b src/main.rs:1 --bundle -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap session-smoke target/debug/fcs -b src/main.rs:1 --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap from-trace "$TRACE_SESSION_NAME" target/debug/fcs --name "${DAP_PROFILE_NAME}-trace" --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap save-profile "$DAP_PROFILE_NAME" target/debug/fcs -b src/main.rs:1 --directory "$SMOKE_ROOT" -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap profiles "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap request-profile "$DAP_PROFILE_NAME" --directory "$SMOKE_ROOT" --bundle

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs man --stdout >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs man --out-dir "$SMOKE_ROOT/man"
rtk scripts/install-local.sh --dry-run --prefix "$SMOKE_ROOT/install"

echo "fcs smoke passed"
