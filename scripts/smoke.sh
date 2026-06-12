#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v rtk >/dev/null 2>&1; then
	rtk() {
		"$@"
	}
fi

export XDG_CACHE_HOME="/tmp/fcs-smoke-cache-${RANDOM}-$$"
export XDG_CONFIG_HOME="/tmp/fcs-smoke-config-${RANDOM}-$$"
PROFILE_NAME="fcs-smoke-$$"
DAP_PROFILE_NAME="fcs-dap-smoke-$$"
TRACE_SESSION_NAME="fcs-trace-smoke-$$"
WORKSPACE_PROFILE_NAME="fcs-workspace-smoke-$$"
QUERY_NAME="fcs-query-smoke-$$"
SMOKE_ROOT="/tmp/fcs-smoke-workspace-${RANDOM}-$$"

rtk mkdir -p "$SMOKE_ROOT"
rtk mkdir -p "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME"
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
rtk tee "$SMOKE_ROOT/tui-script.fcs" >/dev/null <<'EOF'
source symbols
query main
select 1
preview down
break
dap smoke
wait 1000
source debug
EOF

rtk cargo test
rtk cargo clippy -- -D warnings
rtk cargo build

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs complete bash >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs complete zsh >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs tui --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs tui-script "$SMOKE_ROOT/tui-script.fcs" "$SMOKE_ROOT" --mode symbols --query main --format json --step-timeout-ms 10000 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace status
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace plan
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace advise
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace detect
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace advise "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace plan "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace workflows "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor-bundle "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor-bundle "$SMOKE_ROOT" --out "$SMOKE_ROOT/doctor-bundle.txt"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace detect "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace config-schema --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace config-schema --format text >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace config-doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile save "$WORKSPACE_PROFILE_NAME" "$SMOKE_ROOT" --description "smoke profile" --index-root "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile list
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile show "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile use "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile current
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index build "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index stats "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index shards "$SMOKE_ROOT" --target-symbols 2 --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index shards "$SMOKE_ROOT" --target-symbols 2 --format json --write >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index shard-status "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index shard-query main "$SMOKE_ROOT" --kind symbols --limit 5 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index compact "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index prewarm "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index refresh "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index daemon "$SMOKE_ROOT" --interval-ms 0 --max-cycles 1 --foreground
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index daemon-status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index verify "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index list "$SMOKE_ROOT" --kind symbols --limit 5
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index query main "$SMOKE_ROOT" --kind symbols --limit 5 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index profile main "$SMOKE_ROOT" --kind symbols --limit 5 --format json --warn-ms 10000 >/dev/null
rtk tee -a "$SMOKE_ROOT/main.c" >/dev/null <<'EOF'

static int smoke_added_symbol(void)
{
	return 7;
}
EOF
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index refresh "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index query smoke_added_symbol "$SMOKE_ROOT" --kind symbols --limit 5
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index repair "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs index bench "$SMOKE_ROOT" --limit 5 --query main
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function lang:c text:main" "$SMOKE_ROOT" --source all --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "source:index kind:function name:main" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function (name:main or name:smoke_added_symbol) not path:target" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --profile --format json --warn-ms 10000 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function name:smoke_added_symbol" "$SMOKE_ROOT" --source index --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "name:smoke_.*" "$SMOKE_ROOT" --source index --mode regex --macro functions --limit 10 --score-explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function name:main" --source index --mode exact --save "$QUERY_NAME" --limit 1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query --list-saved
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query --use "$QUERY_NAME" --source index --mode exact --limit 1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query --delete-saved "$QUERY_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function text:main" "$SMOKE_ROOT" --source auto --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "kind:function name:main" "$SMOKE_ROOT" --source semantic --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs bench search main "$SMOKE_ROOT" --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs bench index "$SMOKE_ROOT" --format json --limit 5 --query main --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs bench trace --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs bench preview "$SMOKE_ROOT/main.c:8" --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs bench all "$SMOKE_ROOT" --format json --limit 5 --query main --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs files --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs symbol --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs type-def --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs doc-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs outgoing --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs hover --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp health "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp outline --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp breadcrumbs --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp semantic-tokens --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs lsp organize-imports --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph imports "$SMOKE_ROOT" --limit 5 --format text
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph modules "$SMOKE_ROOT" --limit 5 --format dot --depth 2
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph calls "$SMOKE_ROOT" --limit 5 --format json --fanout 4
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format text --fallback index
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format json --fallback index --cache --refresh-cache >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format json --fallback index --cache >/dev/null
if rtk clangd --version >/dev/null 2>&1; then
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format text
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs query "name:main" "$SMOKE_ROOT" --source semantic --limit 10
fi

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace export --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:8:5" --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace note latest "smoke note"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace status latest open
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace priority latest high
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --session "$TRACE_SESSION_NAME" --tag smoke --fallback index --cache --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:4:2" --session "${TRACE_SESSION_NAME}-next" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace list --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace report "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace timeline "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace replay "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace replay-plan "$TRACE_SESSION_NAME" --directory "$SMOKE_ROOT" --format json --program target/debug/fcs --name "${DAP_PROFILE_NAME}-replay"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace structured "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace insights "$TRACE_SESSION_NAME" --directory "$SMOKE_ROOT" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace diff "$TRACE_SESSION_NAME" "${TRACE_SESSION_NAME}-next" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace archive "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions --archived
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace unarchive "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service start "$SMOKE_ROOT" --interval-ms 0 --max-cycles 1 --foreground
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service snapshot "$SMOKE_ROOT" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service query "kind:function text:main" "$SMOKE_ROOT" --source index --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service query "kind:function name:main" "$SMOKE_ROOT" --source index --mode exact --limit 10 --format json --score-explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service query "kind:function text:main" "$SMOKE_ROOT" --source auto --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs service stop "$SMOKE_ROOT"

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions list "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions templates
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions init make-test --directory "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs actions run smoke --directory "$SMOKE_ROOT" --file "$SMOKE_ROOT/main.c" --line 8 --symbol main --dry-run -- --extra
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin list "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin show builtin-dev --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin doctor "$SMOKE_ROOT" --strict
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin schema --format toml >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin templates "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin commands "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin init builtin-dev:rust-debug --directory "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin run builtin-dev:cargo-check --directory "$SMOKE_ROOT" --dry-run --var mode=debug -- --locked
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs plugin plan builtin-dev:cargo-check --directory "$SMOKE_ROOT" --var mode=debug -- --locked

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug command target/debug/fcs -b src/main.rs:1 --cwd . --env FCS_SMOKE=1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug save-profile "$PROFILE_NAME" target/debug/fcs -b src/main.rs:1 --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug from-trace "$TRACE_SESSION_NAME" target/debug/fcs --name "${PROFILE_NAME}-trace" --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug profiles "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug disable-breakpoint "$PROFILE_NAME" 1 --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug enable-breakpoint "$PROFILE_NAME" 1 --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug run-profile "$PROFILE_NAME" --directory "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug delete-profile "$PROFILE_NAME" --directory "$SMOKE_ROOT"

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap launch target/debug/fcs -b src/main.rs:1 --bundle -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap launch target/debug/fcs --request attach --process-id $$ --bundle
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap adapters
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap adapters --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap templates
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap templates --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap session-smoke target/debug/fcs -b src/main.rs:1 --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap session-smoke target/debug/fcs --request attach --process-id $$ --cwd . --env FCS_SMOKE=1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap from-trace "$TRACE_SESSION_NAME" target/debug/fcs --name "${DAP_PROFILE_NAME}-trace" --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap save-profile "$DAP_PROFILE_NAME" target/debug/fcs -b src/main.rs:1 --directory "$SMOKE_ROOT" -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap profiles "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap doctor "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap doctor "$SMOKE_ROOT" --name "$DAP_PROFILE_NAME" --format text
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dap request-profile "$DAP_PROFILE_NAME" --directory "$SMOKE_ROOT" --bundle

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs workspace doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs workspace profile delete "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs man --stdout >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs man --out-dir "$SMOKE_ROOT/man"
rtk scripts/install-local.sh --dry-run --prefix "$SMOKE_ROOT/install"

echo "fcs smoke passed"
