#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SMOKE_TIER="${1:-full}"
case "$SMOKE_TIER" in
	fast | full | release) ;;
	*)
		echo "usage: scripts/smoke.sh [fast|full|release]" >&2
		exit 2
		;;
esac

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
layout debug
assert layout debug
filter kind function
assert filter kind=function
group path
assert group path
filter clear
assert filter none
group none
assert group none
trace view timeline
assert trace-view timeline
trace view graph
assert trace-view graph
source symbols
select 1
preview down
break
dap smoke
wait 1000
source debug
assert results >= 1
assert breakpoints >= 1
assert pending none
EOF
rtk tee "$SMOKE_ROOT/tui-default.fcs" >/dev/null <<'EOF'
assert source files
assert layout search
assert query empty
assert status-level info
assert pending none
assert results >= 1
assert preview-title contains Preview
query __fcs_no_match__
assert query __fcs_no_match__
assert results = 0
assert preview-message contains No selection
assert pending none
EOF
rtk tee "$SMOKE_ROOT/semantic-targets.txt" >/dev/null <<EOF
$SMOKE_ROOT/main.c:8:5
$SMOKE_ROOT/main.c:4:2
EOF

if [[ "$SMOKE_TIER" != "fast" ]]; then
	rtk cargo test
	rtk cargo clippy -- -D warnings
fi
rtk cargo build
if [[ "$SMOKE_TIER" == "release" ]]; then
	rtk cargo build --release
fi

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev complete bash >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev complete zsh >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs ui open --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs ui script "$SMOKE_ROOT/tui-default.fcs" "$SMOKE_ROOT" --format json --step-timeout-ms 10000 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs ui script "$SMOKE_ROOT/tui-script.fcs" "$SMOKE_ROOT" --mode symbols --query main --format json --step-timeout-ms 10000 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project status
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project plan
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project advise
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project detect
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project doctor
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project advise "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project plan "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project workflows "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project doctor-bundle "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project doctor-bundle "$SMOKE_ROOT" --out "$SMOKE_ROOT/doctor-bundle.txt"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project detect "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project config-schema --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project config-schema --format text >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project config-doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project config-migrate "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile save "$WORKSPACE_PROFILE_NAME" "$SMOKE_ROOT" --description "smoke profile" --index-root "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile list
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile show "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile use "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile current
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index build "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index verify "$SMOKE_ROOT" --format text >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index stats "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index shards "$SMOKE_ROOT" --target-symbols 2 --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index shards "$SMOKE_ROOT" --target-symbols 2 --format json --write >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index shard-status "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index shard-query main "$SMOKE_ROOT" --kind symbols --limit 5 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index compact "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index prewarm "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index refresh "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index verify "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index daemon "$SMOKE_ROOT" --interval-ms 0 --max-cycles 1 --foreground
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index daemon-status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index verify "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index list "$SMOKE_ROOT" --kind symbols --limit 5
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index query main "$SMOKE_ROOT" --kind symbols --limit 5 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index profile main "$SMOKE_ROOT" --kind symbols --limit 5 --format json --warn-ms 10000 >/dev/null
rtk tee -a "$SMOKE_ROOT/main.c" >/dev/null <<'EOF'

static int smoke_added_symbol(void)
{
	return 7;
}
EOF
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index refresh "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index query smoke_added_symbol "$SMOKE_ROOT" --kind symbols --limit 5
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index repair "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project index bench "$SMOKE_ROOT" --limit 5 --query main
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function lang:c text:main" "$SMOKE_ROOT" --source all --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "source:index kind:function name:main" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function (name:main or name:smoke_added_symbol) not path:target" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --timing --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --profile --format json --warn-ms 10000 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function name:smoke_added_symbol" "$SMOKE_ROOT" --source index --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "name:smoke_.*" "$SMOKE_ROOT" --source index --mode regex --macro functions --limit 10 --score-explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function name:main" --source index --mode exact --save "$QUERY_NAME" --limit 1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query --list-saved
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query --use "$QUERY_NAME" --source index --mode exact --limit 1
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query --delete-saved "$QUERY_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function text:main" "$SMOKE_ROOT" --source auto --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "kind:function name:main" "$SMOKE_ROOT" --source semantic --limit 10
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench search main "$SMOKE_ROOT" --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench index "$SMOKE_ROOT" --format json --limit 5 --query main --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench trace --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench tui "$SMOKE_ROOT" --query main --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench preview "$SMOKE_ROOT/main.c:8" --format json --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench all "$SMOKE_ROOT" --format json --limit 5 --query main --warn-ms 10000
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench baseline "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev bench compare "$SMOKE_ROOT" --format json --strict >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find files --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code type --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code doc-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code outgoing --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code hover --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code workspace-symbols --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code health "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code outline --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code breadcrumbs --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code tokens --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code organize-imports --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph imports "$SMOKE_ROOT" --limit 5 --format text
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph modules "$SMOKE_ROOT" --limit 5 --format dot --depth 2
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph calls "$SMOKE_ROOT" --limit 5 --format json --fanout 4
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format text --fallback index
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format json --fallback index --cache --refresh-cache >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format json --fallback index --cache >/dev/null
if rtk clangd --version >/dev/null 2>&1; then
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs code graph semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --format text
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs find query "name:main" "$SMOKE_ROOT" --source semantic --limit 10
fi

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace export --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format mermaid >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format dot >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace use "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace current
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:8:5" --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace note latest "smoke note"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace status latest open
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace priority latest high
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace semantic "$SMOKE_ROOT/main.c:8:5" --directory "$SMOKE_ROOT" --relation outgoing --session "$TRACE_SESSION_NAME" --tag smoke --fallback index --cache --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace semantic --from-query "kind:function name:main" --directory "$SMOKE_ROOT" --query-source index --query-limit 2 --relation references --tag smoke --fallback index --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace semantic --targets-file "$SMOKE_ROOT/semantic-targets.txt" --directory "$SMOKE_ROOT" --relation outgoing --session "$TRACE_SESSION_NAME" --tag smoke --fallback index --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:4:2" --session "${TRACE_SESSION_NAME}-next" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace list --session "$TRACE_SESSION_NAME" --tag smoke
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format json --session "$TRACE_SESSION_NAME" --tag smoke --collapse-threshold 1 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format mermaid --session "$TRACE_SESSION_NAME" --relation outgoing --collapse-threshold 1 >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace graph --format dot --session "$TRACE_SESSION_NAME" --kind bookmark >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace report "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace timeline "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace replay "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace replay-plan "$TRACE_SESSION_NAME" --directory "$SMOKE_ROOT" --format json --program target/debug/fcs --name "${DAP_PROFILE_NAME}-replay"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace structured "$TRACE_SESSION_NAME" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace insights "$TRACE_SESSION_NAME" --directory "$SMOKE_ROOT" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace diff "$TRACE_SESSION_NAME" "${TRACE_SESSION_NAME}-next" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace diff "$TRACE_SESSION_NAME" "${TRACE_SESSION_NAME}-next" --format json --filter semantic >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:4:2" --session "${TRACE_SESSION_NAME}-edit-a" --tag edit
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace add "$SMOKE_ROOT/main.c:8:5" --session "${TRACE_SESSION_NAME}-edit-b" --tag keep
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace rename "${TRACE_SESSION_NAME}-edit-a" "${TRACE_SESSION_NAME}-edit-renamed"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace merge "${TRACE_SESSION_NAME}-edit-renamed" "${TRACE_SESSION_NAME}-edit-b"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace split "${TRACE_SESSION_NAME}-edit-b" "${TRACE_SESSION_NAME}-edit-split" --tag edit
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace verify --directory "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace repair --directory "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace compact --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace archive "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace sessions --archived
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs trace unarchive "$TRACE_SESSION_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service start "$SMOKE_ROOT" --interval-ms 0 --max-cycles 1 --foreground
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service status "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service snapshot "$SMOKE_ROOT" --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service query "kind:function text:main" "$SMOKE_ROOT" --source index --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service query "kind:function name:main" "$SMOKE_ROOT" --source index --mode exact --limit 10 --format json --score-explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service query "source:index kind:function text:main" "$SMOKE_ROOT" --source all --limit 10 --explain
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service query "kind:function text:main" "$SMOKE_ROOT" --source auto --limit 10 --format json
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project service stop "$SMOKE_ROOT"

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project action list "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project action templates
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project action init make-test --directory "$SMOKE_ROOT" --dry-run
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project action doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project action run smoke --directory "$SMOKE_ROOT" --file "$SMOKE_ROOT/main.c" --line 8 --symbol main --dry-run -- --extra
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

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap launch target/debug/fcs -b src/main.rs:1 --bundle -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap launch target/debug/fcs -b src/main.rs:1 --break-condition "argc > 0" --break-hit 1 --break-log "hit main" --bundle -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap launch target/debug/fcs --request attach --process-id $$ --bundle
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap adapters
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap adapters --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap templates
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap templates --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap session-smoke target/debug/fcs -b src/main.rs:1 --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap session-smoke target/debug/fcs -b src/main.rs:1 --break-condition "argc > 0" --break-hit 1 --break-log "hit main" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap session-smoke target/debug/fcs --request attach --process-id $$ --cwd . --env FCS_SMOKE=1
if [[ "${FCS_REAL_DAP_SMOKE:-0}" == "1" ]]; then
	echo "running real DAP smoke; this requires an adapter on PATH and ptrace/debug permission" >&2
	rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap adapter-session auto target/debug/fcs --cwd . --format json --request-timeout-ms 30000 --event-timeout-ms 15000 --max-read-frames 256 -- --help >/dev/null
fi
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap from-trace "$TRACE_SESSION_NAME" target/debug/fcs --name "${DAP_PROFILE_NAME}-trace" --directory "$SMOKE_ROOT" --cwd . --env FCS_SMOKE=1 -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap save-profile "$DAP_PROFILE_NAME" target/debug/fcs -b src/main.rs:1 --directory "$SMOKE_ROOT" -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap save-profile "${DAP_PROFILE_NAME}-advanced" target/debug/fcs -b src/main.rs:1 --break-condition "argc > 0" --break-hit 1 --break-log "hit main" --directory "$SMOKE_ROOT" -- --help
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap profiles "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap doctor "$SMOKE_ROOT" --format json >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap doctor "$SMOKE_ROOT" --name "$DAP_PROFILE_NAME" --format text
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap request-profile "$DAP_PROFILE_NAME" --directory "$SMOKE_ROOT" --bundle
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs debug dap transcript "$DAP_PROFILE_NAME" --directory "$SMOKE_ROOT" --format json >/dev/null

rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs project doctor "$SMOKE_ROOT"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" XDG_CONFIG_HOME="$XDG_CONFIG_HOME" target/debug/fcs project profile delete "$WORKSPACE_PROFILE_NAME"
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev man --stdout >/dev/null
rtk env XDG_CACHE_HOME="$XDG_CACHE_HOME" target/debug/fcs dev man --out-dir "$SMOKE_ROOT/man"
rtk scripts/install-local.sh --dry-run --prefix "$SMOKE_ROOT/install"

echo "fcs smoke passed"
