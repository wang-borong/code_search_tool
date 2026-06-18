# fcs 使用手册

本文是 `fcs` 的完整操作手册，面向日常代码搜索、语义追踪、调试准备、大型仓库巡检和发布前验证。README 保留项目概览和常用示例；需要逐项查功能时，以本文为入口。

命令示例默认假设 `fcs` 已在 `PATH` 中。若在本仓库内用 agent 验证命令，可按仓库约定在命令前加 `rtk`。

## 1. 快速开始

### 1.1 编译与验证

```bash
cargo build
cargo test
target/debug/fcs --help
```

安装后检查版本和命令树：

```bash
fcs --help
fcs man --stdout
```

生成 shell completion：

```bash
fcs complete zsh > ~/.zsh/completion/_fcs
fcs complete bash > ~/.local/share/bash-completion/completions/fcs
```

生成 man page：

```bash
fcs man --out-dir ~/.local/share/man/man1
man fcs
```

### 1.2 第一次进入项目

建议先在项目根目录执行以下命令：

```bash
fcs workspace status .
fcs workspace advise .
fcs index refresh .
fcs index status .
fcs tui .
```

每条命令的用途：

- `workspace status`：检查 workspace root、缓存目录、LSP 和基础工具是否可用。
- `workspace advise`：给出项目类型、建议的初始化步骤和健康提示。
- `index refresh`：在缺失、过期或 schema 迁移时刷新文件与符号索引。
- `index status`：确认缓存是否新鲜、文件数和符号数是否符合预期。
- `tui`：进入常驻代码追踪工作台。

### 1.3 典型 5 分钟工作流

```bash
fcs tui . --mode symbols --query main
```

在 TUI 内：

1. 用 `/` 修改查询。
2. 用 `Tab` / `Shift-Tab` 切换 `Search`、`Files`、`Symbols`、`Trace`、`Debug` 等 source。
3. 用 `j` / `k` 或方向键移动结果。
4. 用 `Enter` / `o` 打开当前结果并写入 trace。
5. 用 `gd`、`gr`、`t`、`i`、`c`、`C`、`e` 执行 LSP 语义跳转。
6. 用 `a` 把当前位置加入 trace bookmark。
7. 用 `b` 加入断点列表，`D` 切换到 Debug source。
8. 用 `:` 打开命令面板，执行更细的 trace、debug、DAP 和 preview 操作。

如果只想一次性搜索：

```bash
fcs search "fn main" .
fcs files . --query main
fcs symbol . --query parse_config
fcs preview src/main.rs:120:30
```

## 2. 核心概念

### 2.1 Workspace

workspace 是一次代码调查的根目录。`fcs` 会围绕 workspace 保存缓存、索引、trace、debug profile、DAP profile 和 TUI 状态。

常用命令：

```bash
fcs workspace status .
fcs workspace init .
fcs workspace detect .
fcs workspace doctor .
fcs workspace doctor-bundle . --format json --out /tmp/fcs-doctor.json
```

`workspace init` 只写入非侵入式缓存元数据，不会重写源码。需要项目级配置时使用：

```bash
fcs workspace config .
fcs workspace config-doctor .
fcs workspace config-schema --format toml
fcs workspace config-migrate . --dry-run
```

### 2.2 Location 格式

多数语义、trace、debug 和 preview 命令都接受以下格式：

```text
path
path:line
path:line:column
```

示例：

```bash
fcs preview src/main.rs:42:20
fcs def src/main.rs:42:5
fcs trace add src/main.rs:42 --session bug-42
fcs debug command target/debug/app -b src/main.rs:42
```

行号和列号都是面向用户的 1-based 数值。只提供行号时，语义命令会使用该行附近的默认列。

### 2.3 Index 与 Shard

index 是 workspace 的持久化文件和轻量符号缓存，适合大仓库快速查询。shard 是大仓库的分片缓存，用于降低单次加载和查询成本。

基础命令：

```bash
fcs index refresh .
fcs index list . --kind symbols --limit 50
fcs index query parse_config . --kind symbols --limit 20 --timing
fcs index verify . --format json
```

大仓库建议：

```bash
fcs index shards . --target-symbols 5000 --format json
fcs index shards . --target-symbols 5000 --write
fcs index shard-status . --format json
fcs index shard-query parse_config . --kind symbols --limit 20 --timing
```

### 2.4 Trace Session

trace session 是一次调查的路径记录。它可以保存书签、语义边、debug stop、状态、优先级、标签和父子关系。

```bash
fcs trace use bug-42
fcs trace add src/main.rs:42 --session bug-42 --tag hot
fcs trace list --session bug-42 --tag hot
fcs trace report bug-42 --format markdown
fcs trace graph --session bug-42 --format mermaid
```

### 2.5 LSP 与 Fallback

LSP 提供 definition、references、type definition、implementation、diagnostics、hover、call hierarchy 等语义能力。C/C++ 通常使用 `clangd`，Rust 通常使用 `rust-analyzer`。

语义能力不可用时，部分命令可以用 index fallback 降级：

```bash
fcs graph semantic src/main.rs:42:5 --relation outgoing --fallback index
fcs trace semantic src/main.rs:42:5 --relation references --fallback index
fcs query "name:parse_config" . --source semantic
```

fallback 结果适合继续排查，但它不是完整的编译级语义结果。

### 2.6 Debug 与 DAP

`debug` 面向传统 `gdb` / `lldb` 命令生成和 profile 管理。`dap` 面向 Debug Adapter Protocol，可生成请求、保存 profile、运行 mock smoke，也可对真实 adapter 执行非交互 session。

Arch Linux 上官方 `lldb` 包通常提供 `/usr/bin/lldb-dap`。没有单独的 `lldb-dap` 包不代表缺功能；AUR `codelldb` 是可选 adapter。

## 3. TUI 工作台

### 3.1 启动方式

```bash
fcs tui
fcs tui .
fcs tui . --mode files --query main
fcs tui . --mode symbols --query handle
fcs tui . --debug-binary target/debug/app
```

`--mode` 可指定初始 source，常用值包括：

- `search`：全文搜索 source。
- `files`：文件查找 source。
- `symbols`：轻量符号 source，优先走缓存和 sidecar。
- `refs`：引用结果 source。
- `diag`：诊断结果 source。
- `trace`：trace 历史 source。
- `pinned`：已固定结果 source。
- `debug`：断点、debug profile 和 DAP 状态 source。

### 3.2 界面区域

TUI 主要由这些区域组成：

- Source 列表：展示当前数据源和分组。
- Results 面板：展示搜索、文件、符号、语义、trace 或 debug 结果。
- Preview 面板：展示当前位置附近代码，并高亮匹配命中。
- Trace / Debug 面板：显示当前调查路径、断点和 profile。
- Activity / Status 区域：显示后台 worker、LSP、DAP、错误和快捷键提示。

当结果很多时，优先使用 `query`、`filter`、`group` 缩小范围，而不是只依赖滚动。

### 3.3 基础快捷键

- `q` / `Esc` / `Ctrl-C`：退出并恢复终端。
- `/`：进入 query 输入模式。
- `Enter`：在输入模式提交 query；在结果模式打开当前结果。
- `Tab` / `Shift-Tab`：切换 source。
- `j` / `k` / 方向键：移动结果选择。
- `o`：打开当前结果并写入 trace。
- `[` / `]`：在 TUI 导航栈后退/前进。
- `?`：显示或刷新状态栏快捷键提示。
- `P`：锁定或解锁 preview。
- `PageUp` / `PageDown`：滚动 preview。

### 3.4 语义快捷键

- `gd`：跳转 definition。
- `gr`：列出 references。
- `t` / `gt`：跳转 type definition。
- `i` / `gi`：列出 implementation。
- `s`：显示当前文件 document symbols。
- `c`：显示 incoming calls。
- `C`：显示 outgoing calls。
- `e`：显示当前文件 diagnostics。

语义请求由后台 LSP worker 执行。LSP 超时或不可用时，TUI 会把错误显示在状态栏，不会阻塞主界面。

### 3.5 Trace 与 Debug 快捷键

- `a`：把当前结果加入 trace bookmark。
- `b`：把当前结果加入断点列表。
- `B`：把 trace 中的位置批量加入断点列表。
- `D`：切换到 Debug source。
- `X`：显式启动 Debug source 中的 gdb/lldb 会话；选中 profile 时运行该 profile。
- `x`：在 Debug source 中删除当前 profile 或断点。

DAP worker 快捷键：

- `F5`：continue。
- `F6`：pause。
- `F10`：next。
- `F11`：step in。
- `Shift-F11`：step out。
- `Ctrl-F5`：stop。

### 3.6 命令面板

按 `:` 打开命令面板。支持 `Tab` 补全和 `Up` / `Down` 历史。

Source 与查询：

```text
source search
source files
source symbols
source trace
source debug
query parse_config
refresh
open
quit
```

布局：

```text
layout search
layout debug
layout trace
layout semantic
layout balanced
```

过滤与分组：

```text
filter kind function
filter path src/
filter text parse
filter clear
group kind
group path
group none
```

Preview：

```text
preview lock
preview up
preview down
preview reset
```

语义操作：

```text
def
refs
type
impl
symbols
diag
incoming
outgoing
hover
```

Trace：

```text
trace session bug-42
trace current
trace sessions
trace view session
trace view timeline
trace view graph
trace semantic outgoing
trace semantic references
trace breakpoint
trace dap-profile bug-42-dap
```

Debug 与 DAP：

```text
break sync
debug
run
dap start smoke
dap real auto
dap sync
dap restart
dap terminate
dap disconnect
dap adapters
dap jump
dap open
```

Watch、Eval 和变量树：

```text
watch add argc
watch del argc
watch clear
watch refresh
eval argc
var page 2
var next
var prev
```

状态：

```text
status copy
status health
```

`status copy` 用于把最近错误或状态文本输出成可复制内容，适合贴到 issue 或调查记录里。

### 3.7 TUI Script

`tui-script` 用于无交互回放 TUI 命令，适合回归测试、录制调查步骤和 CI smoke。

```bash
fcs tui-script trace-loop.fcs . --mode symbols --query main --format json
fcs tui-script trace-loop.fcs . --step-timeout-ms 3000 --persist
```

脚本规则：

- 空行和 `#` 开头的行会被忽略。
- 命令面板语法可直接写入脚本。
- 支持 `select <n>` 选择第 n 个结果。
- 支持 `move <delta>` 相对移动。
- 支持 `wait <ms>` 等待后台 worker。
- 支持 `assert ...` 断言。

常用断言：

```text
assert results >= 1
assert status contains traced
assert trace-session bug-42
assert trace-view graph
assert layout debug
assert filter none
assert group path
assert pending none
```

默认不会持久化 TUI state。确实需要写回 pins、breakpoints、navigation、session、layout、filter 或 group 时加 `--persist`。

## 4. 搜索、文件、符号与预览

### 4.1 全文搜索

```bash
fcs search "fn main"
fcs search "struct Config" /path/to/project
fcs search main install.sh README.md src
fcs search "TODO" -o --no-ignore -o -i
```

`search` 的第一个参数是正则 pattern，后续 `PATH` 是搜索范围。`-o` / `--option` 可传递 ripgrep 兼容参数，例如：

```bash
fcs search "handle_" src -o -i
fcs search "unsafe" . -o --hidden -o --no-ignore
fcs search "main" . -o -g -o "*.rs"
```

### 4.2 文件查找

```bash
fcs files
fcs files .
fcs files /path/to/project --query main
fcs files . -o --hidden -o --no-ignore
```

`files` 适合一次性选择文件。需要连续调查时，优先使用 `fcs tui --mode files`。

### 4.3 符号查找

```bash
fcs symbol
fcs symbol .
fcs symbol /path/to/project --query parse_config
fcs symbol . -o --max-depth=3
```

`symbol` 是轻量符号提取，不依赖 LSP。它适合快速粗定位函数、结构体、类型、宏和测试入口。需要编译级准确跳转时，用 LSP 命令或 TUI 的 `gd` / `gr`。

### 4.4 单文件预览

```bash
fcs preview src/main.rs:100
fcs preview src/main.rs:100:30
```

格式为 `path:line[:height]`。`height` 控制展示窗口高度，适合在脚本或终端里快速看上下文。

### 4.5 Ignore 管理

```bash
fcs ignore init
fcs ignore add target/ node_modules/ "*.log"
fcs ignore list
fcs ignore remove "*.log"
```

默认作用于当前目录。需要指定目录：

```bash
fcs ignore --directory /path/to/project list
```

### 4.6 Query History

```bash
fcs history list
fcs history clear
```

history 记录交互查询，便于复用常查表达式。保存复杂字段查询时，优先用 `fcs query --save`。

## 5. Workspace、Service 与 Index

### 5.1 Workspace 健康检查

```bash
fcs workspace status .
fcs workspace advise .
fcs workspace plan .
fcs workspace workflows . --format text
fcs workspace doctor .
fcs workspace doctor-bundle . --format json --out /tmp/fcs-doctor.json
```

用途建议：

- `status`：日常检查 readiness。
- `advise`：首次接入项目时查看建议。
- `plan`：查看 TUI 启动时的非阻塞初始化计划。
- `workflows`：查看适合当前项目的诊断流程模板。
- `doctor`：汇总 workspace、cache、config 和 release 健康状况。
- `doctor-bundle`：生成支持包，适合发给维护者。

### 5.2 Workspace Profile

大型 monorepo 常有多个常用根目录。profile 用于保存这些入口。

```bash
fcs workspace profile save core . --description "core workspace" --index-root src
fcs workspace profile list
fcs workspace profile show core
fcs workspace profile use core
fcs workspace profile current
fcs workspace profile delete core
```

`--index-root` 可重复传入，用于限制或组合索引根。

### 5.3 Background Service

service 用于刷新 index 并写出统一状态快照。

```bash
fcs service start . --interval-ms 2000 --foreground
fcs service start . --interval-ms 2000 --max-cycles 1 --foreground
fcs service status .
fcs service snapshot . --format json
fcs service stop .
```

通过 service 查询：

```bash
fcs service query "kind:function text:main" . --source index --mode exact --score-explain
```

当你希望编辑器、脚本或外部工具读取统一 workspace 状态时，用 service 比反复启动多个命令更稳定。

### 5.4 Index 基础操作

```bash
fcs index build .
fcs index refresh .
fcs index status .
fcs index stats .
fcs index list . --kind files --limit 50
fcs index list . --kind symbols --limit 50
```

`build` 总是重建。`refresh` 只在缺失、过期、损坏或 schema 迁移时重建。

### 5.5 Index 查询、诊断与修复

```bash
fcs index query main . --kind symbols --limit 20 --timing --warn-ms 200
fcs index profile main . --kind symbols --limit 20 --format json --warn-ms 200
fcs index verify . --format json
fcs index doctor .
fcs index repair .
fcs index repair . --force
fcs index compact . --dry-run
fcs index prewarm .
```

建议：

- 查询慢时先用 `profile` 看加载、列表、查询和 shard 耗时。
- 不确定缓存是否健康时用 `verify`。
- 发现 stale 或 corrupt 时用 `repair`。
- 大型仓库首次进入 TUI 前可用 `prewarm` 预热文件系统缓存。

### 5.6 Shard 与 Daemon

```bash
fcs index shards . --target-symbols 5000 --format json
fcs index shards . --target-symbols 5000 --write
fcs index shard-status . --format json
fcs index shard-query parse_config . --kind symbols --limit 20 --timing
fcs index daemon . --interval-ms 2000 --foreground
fcs index daemon . --interval-ms 2000 --max-cycles 1 --foreground
fcs index daemon-status .
```

大仓库推荐流程：

1. `index refresh` 建主索引。
2. `index shards --format json` 查看建议分片。
3. `index shards --write` 写入 shard manifest 和缓存。
4. `index shard-status` 检查新鲜度。
5. TUI 和 `query` 走 streaming / sidecar backed source 时，可显著降低大结果集的交互压力。

## 6. 统一查询与 Benchmark

### 6.1 Query 表达式

`fcs query` 可以同时查询 index、trace 和 semantic source。常用字段：

- `kind:`：符号种类，例如 `function`、`struct`、`file`。
- `lang:` / `language:`：语言。
- `path:`：路径片段。
- `name:`：符号名。
- `text:`：通用文本。
- `source:`：数据源。
- `status:`：trace 状态。
- `priority:`：trace 优先级。
- `session:`：trace session。
- `tag:`：trace 标签。

示例：

```bash
fcs query "kind:function lang:rust text:parse" . --source all --format json
fcs query "path:src name:main" . --source index --limit 20
fcs query "session:bug-42 status:open tag:hot" . --source trace
fcs query "source:index kind:function name:main" . --source all --explain
fcs query "kind:function (name:parse or name:init) not path:target" . --source all --explain
```

### 6.2 Matching Mode、Macro 与 Saved Query

```bash
fcs query "name:parse_.*" . --source index --mode regex --macro functions --score-explain
fcs query "kind:function name:parse_config" . --source index --mode exact
fcs query "kind:function text:main" . --source all --timing --warn-ms 200
fcs query "kind:function text:main" . --source all --profile --format json --warn-ms 200
```

保存和复用：

```bash
fcs query "kind:function name:parse_config" . --source index --mode exact --save parse-config
fcs query --use parse-config --source index --mode exact
fcs query --list-saved
fcs query --delete-saved parse-config
```

source 选择：

```bash
fcs query "name:parse_config" . --source semantic
fcs query "kind:function text:main" . --source auto
```

`--source semantic` 会优先使用 LSP workspace/symbol。LSP 配置缺失或查询失败时，会返回带 `fallback:index:*` 来源前缀的 index 结果。

### 6.3 Benchmark

```bash
fcs bench search main . --format json --warn-ms 200
fcs bench index . --limit 50 --query main --warn-ms 200
fcs bench tui . --query main --format json
fcs bench trace --format json
fcs bench preview src/main.rs:20 --warn-ms 20
fcs bench all . --query main --limit 50
```

保存和比较基线：

```bash
fcs bench baseline .
fcs bench compare . --format json --threshold-ms 10 --threshold-percent 25 --strict
```

建议把 `bench all`、`bench baseline`、`bench compare --strict` 放到性能敏感改动后的验证流程里。

## 7. LSP 语义导航

### 7.1 基础语义命令

```bash
fcs def src/main.rs:42:5
fcs refs src/main.rs:42:5
fcs type-def src/main.rs:42:5
fcs implementation src/main.rs:42:5
fcs incoming src/main.rs:42:5
fcs outgoing src/main.rs:42:5
fcs hover src/main.rs:42:5
fcs diag src/main.rs
fcs doc-symbols src/main.rs
fcs workspace-symbols parse_config --limit 50
```

通过 `--directory` 指定 workspace：

```bash
fcs def src/main.rs:42:5 --directory /path/to/workspace
```

### 7.2 LSP 健康与高级命令

```bash
fcs lsp health .
fcs lsp health . --file src/main.rs
fcs lsp highlights src/main.rs:42:5
fcs lsp refs src/main.rs:42:5
fcs lsp outline src/main.rs
fcs lsp breadcrumbs src/main.rs:42:5
fcs lsp semantic-tokens src/main.rs --line 42 --format json
fcs lsp call-tree src/main.rs:42:5
```

重命名和 code actions：

```bash
fcs lsp rename src/main.rs:42:5 new_symbol
fcs lsp rename src/main.rs:42:5 new_symbol --apply --dry-run
fcs lsp rename src/main.rs:42:5 new_symbol --apply
fcs lsp code-actions src/main.rs:42:5 --format json
fcs lsp code-actions src/main.rs:42:5 --apply 1 --dry-run
fcs lsp organize-imports src/main.rs
fcs lsp organize-imports src/main.rs --apply --dry-run
fcs lsp organize-imports src/main.rs --apply
```

涉及写文件的 LSP 操作建议先不加 `--apply` 查看 preview；需要验证写入路径时使用 `--apply --dry-run`；确认后再只加 `--apply` 真正写入。

## 8. Graph 与 Trace

### 8.1 Semantic Graph

```bash
fcs graph semantic src/main.rs:42:5 --relation outgoing --format text
fcs graph semantic src/main.rs:42:5 --relation references --format json
fcs graph semantic src/main.rs:42:5 --relation outgoing --format mermaid --fanout 20
fcs graph semantic src/main.rs:42:5 --relation outgoing --format dot --fallback index --cache
fcs graph semantic src/main.rs:42:5 --relation outgoing --format json --fallback index --cache --refresh-cache
```

参数说明：

- `--relation`：`references`、`definition`、`type`、`implementation`、`incoming`、`outgoing`。
- `--format`：`text`、`json`、`mermaid`、`dot`。
- `--depth`：保留的关系深度。当前语义查询通常是一跳。
- `--fanout`：限制每个源的边数量，`0` 表示不限制。
- `--exclude`：过滤 source、target、kind 或 detail 包含指定文本的边。
- `--fallback index`：LSP 失败或无结果时用 index 降级。
- `--cache`：读取或写入 workspace semantic graph cache。
- `--refresh-cache`：忽略旧 cache 并重写。

### 8.2 Import、Module 与 Call Graph

```bash
fcs graph imports . --limit 100 --format text
fcs graph imports . --limit 100 --depth 2 --fanout 8 --exclude target --format mermaid
fcs graph modules . --limit 100 --depth 2 --format dot
fcs graph calls . --limit 100 --depth 2 --fanout 8 --format json
```

`imports` 适合 C/C++ include、Rust use/mod 和常见脚本导入关系的轻量扫描。`modules` 偏 Rust 模块关系。`calls` 是离线轻量 call graph，不等同于完整 LSP 调用层级。

### 8.3 Trace 基础操作

```bash
fcs trace add src/main.rs:42 -l "init path" --session bug-42 --branch main --tag hot
fcs trace list
fcs trace list --session bug-42 --tag hot --status open
fcs trace note latest "checked failing path"
fcs trace status latest open
fcs trace priority latest high
```

`note`、`status`、`priority` 的 id 可以是 trace id，也可以是 `latest`。传入 `-` 可清空字段。

Session：

```bash
fcs trace sessions
fcs trace sessions --archived
fcs trace use bug-42
fcs trace current
fcs trace archive bug-42
fcs trace unarchive bug-42
```

### 8.4 Trace 语义记录

```bash
fcs trace semantic src/main.rs:42:5 --relation outgoing --session bug-42 --fallback index
fcs trace semantic --targets-file targets.txt --relation references --session bug-42
fcs trace semantic --from-query "kind:function name:init" --query-source index --query-limit 10 --directory .
```

适用场景：

- 从一个函数出发记录调用边。
- 从 query 结果批量展开语义边。
- 在 LSP 不稳定时保留 index fallback 的调查线索。

### 8.5 Trace 导出、比较与修复

```bash
fcs trace report bug-42 --format markdown
fcs trace report bug-42 --format json
fcs trace timeline bug-42 --format json
fcs trace replay bug-42 --format markdown
fcs trace replay-plan bug-42 --program target/debug/app --name bug-42-dap --format json
fcs trace structured bug-42 --format json
fcs trace insights bug-42 --directory . --format markdown
fcs trace diff bug-42 bug-42-next --format json
fcs trace diff bug-42 bug-42-next --format json --filter semantic
fcs trace graph --format mermaid --session bug-42 --tag hot --collapse-threshold 8
```

维护操作：

```bash
fcs trace rename bug-42-old bug-42
fcs trace merge bug-42-spike bug-42
fcs trace split bug-42 bug-42-hot --tag hot
fcs trace verify --directory . --format json --strict
fcs trace repair --directory . --format text
fcs trace compact --format json
fcs trace export --directory . --format json
fcs trace clear
```

## 9. Debug 与 DAP

### 9.1 传统 Debug 命令

生成 gdb/lldb 命令：

```bash
fcs debug command target/debug/app -b src/main.rs:42 --cwd . --env RUST_LOG=debug
fcs debug command target/debug/app -b src/main.rs:42 --debugger lldb -- --config dev.toml
fcs debug command target/debug/app -b src/main.rs:42 --run
```

使用最新 trace 位置：

```bash
fcs debug last target/debug/app
fcs debug last target/debug/app --debugger lldb --run
```

Profile：

```bash
fcs debug save-profile smoke target/debug/app -b src/main.rs:1 --cwd . --env RUST_LOG=debug -- --help
fcs debug profiles
fcs debug run-profile smoke
fcs debug run-profile smoke --run
fcs debug disable-breakpoint smoke 1
fcs debug enable-breakpoint smoke 1
fcs debug delete-profile smoke
```

从 trace 生成 profile：

```bash
fcs debug from-trace bug-42 target/debug/app --name bug-42-debug --cwd . --env RUST_LOG=debug -- --config dev.toml
```

### 9.2 DAP 请求生成

生成 launch 请求：

```bash
fcs dap launch target/debug/app -- --config dev.toml
fcs dap launch target/debug/app -b src/main.rs:42 --bundle -- --config dev.toml
fcs dap launch target/debug/app -b src/main.rs:42 --break-condition "argc > 1" --break-hit 3 --break-log "main hit" --bundle
```

生成 attach 请求：

```bash
fcs dap launch target/debug/app --request attach --process-id 12345
```

保存 profile：

```bash
fcs dap save-profile smoke target/debug/app -b src/main.rs:42 --cwd . --env RUST_LOG=debug -- --config dev.toml
fcs dap profiles
fcs dap request-profile smoke --bundle
fcs dap transcript smoke --format json
```

从 trace 生成 DAP profile：

```bash
fcs dap from-trace bug-42 target/debug/app --name bug-42-dap --cwd . --env RUST_LOG=debug -- --config dev.toml
```

### 9.3 Adapter 发现、诊断与真实 Session

```bash
fcs dap adapters
fcs dap adapters --format json
fcs dap templates
fcs dap templates --format json
fcs dap doctor . --format json
fcs dap doctor . --name smoke --format text
```

Mock smoke：

```bash
fcs dap session-smoke target/debug/app -b src/main.rs:42 -- --config dev.toml
```

真实 adapter session：

```bash
fcs dap adapter-session auto target/debug/app -b src/main.rs:42 --cwd . --format json --request-timeout-ms 30000 --event-timeout-ms 15000 --max-read-frames 256 -- --config dev.toml
fcs dap adapter-session /usr/bin/lldb-dap target/debug/app -b src/main.rs:42 --cwd . --format json -- --config dev.toml
```

Attach 示例：

```bash
fcs dap adapter-session auto /path/to/program --request attach --process-id 12345 --cwd . --format json
```

真实 DAP 注意事项：

- `auto` 会优先选择可用 adapter。
- `lldb-dap` 启动时会默认避免用户级 `.lldbinit` 干扰自动化会话。
- 真实 session 需要普通本机 shell 环境允许调试和 ptrace。
- 受限 sandbox、容器、ptrace 禁止或安全策略过严时，LLDB handshake/launch 失败不一定是 `fcs` bug。
- Arch Linux 优先安装官方 `lldb` 包；AUR `codelldb` 可选，不是必需项。

## 10. Actions 与 Plugins

### 10.1 Project Actions

actions 是可配置的项目命令模板，适合把“对当前符号跑测试”“对当前文件跑 lint”接到 TUI 或 CLI 流程里。

```bash
fcs actions list
fcs actions list /path/to/project
fcs actions templates
fcs actions init rust-cargo-test --dry-run
fcs actions init rust-cargo-test --directory . --force
fcs actions doctor
fcs actions run test-symbol --file src/lib.rs --line 42 --symbol parse_config --dry-run
fcs actions run test-symbol --directory /path/to/project -- --exact
```

支持变量：

- `{workspace}`：workspace 根目录。
- `{file}`：当前文件。
- `{line}`：当前行号。
- `{symbol}`：当前符号名。

### 10.2 Plugins

plugins 是声明式 TOML manifest，不加载动态库。发现路径：

- 内置插件。
- `$XDG_CONFIG_HOME/fcs/plugins/*.toml`。
- `<workspace>/.fcs/plugins/*.toml`。

命令：

```bash
fcs plugin list
fcs plugin show builtin-dev
fcs plugin doctor
fcs plugin doctor --strict
fcs plugin schema --format toml
fcs plugin commands
fcs plugin templates
fcs plugin init builtin-dev:rust-debug --dry-run
fcs plugin run builtin-dev:cargo-check --dry-run --var mode=debug -- --locked
fcs plugin plan builtin-dev:cargo-check --var mode=debug -- --locked
```

插件 commands/templates 支持 actions 的变量，也支持：

- `env = { KEY = "VALUE" }`
- `[[commands.pre]]`
- `[[commands.post]]`
- `{env.NAME}`
- `--var KEY=VALUE` 对应 `{var.KEY}`

`plugin plan` 会打印完整执行计划，不运行命令，适合排查变量展开、pre/post 顺序和最终参数。

## 11. 配置文件

### 11.1 全局配置

首次运行会生成全局配置：

```text
~/.config/fcs/fcs.toml
$XDG_CONFIG_HOME/fcs/fcs.toml
```

关键段落：

```toml
schema_version = 1

[search]
rg_options = []
ignore = [".git/", "target/", "node_modules/", "*.tmp", "*.log"]

[skim]
height = "100%"
exact = true
tac = true
cycle = true
preview_window = "right:59%"

[editor]
command = "nvim"

[lsp]
clangd_command = "clangd"
request_timeout_ms = 3000

[tui.keymap]
command_palette = ":"
query = "/"
open = "o"
refresh = "r"
trace = "a"
breakpoint = "b"
debug = "D"

[tui.theme]
name = "default"
color = true
syntax_highlight = true
low_color = false
```

### 11.2 项目配置

生成项目 `.fcs.toml`：

```bash
fcs workspace config .
fcs workspace config . --force
fcs workspace config-doctor .
fcs workspace config-schema --format toml
fcs workspace config-migrate . --dry-run
```

项目配置适合放：

- 项目专用 `clangd_command`。
- 默认 debug binary。
- 项目搜索忽略项。
- 项目 actions。
- 插件初始化出的 action templates。

配置迁移建议先执行 `--dry-run`，确认报告后再写入。

## 12. 大型仓库建议

### 12.1 首次接入

```bash
fcs workspace doctor /path/to/repo
fcs index refresh /path/to/repo
fcs index verify /path/to/repo --format json
fcs index stats /path/to/repo
fcs bench all /path/to/repo --query main --limit 50
```

检查重点：

- 文件数和符号数是否符合项目规模。
- ignore 是否漏掉 `build/`、`target/`、生成目录或第三方巨型依赖。
- `bench all` 中 search、index、tui、preview 是否有明显异常。
- LSP 是否能在常用文件上返回 definition / references。

### 12.2 分片和 sidecar

```bash
fcs index shards /path/to/repo --target-symbols 5000 --format json
fcs index shards /path/to/repo --target-symbols 5000 --write
fcs index shard-query main /path/to/repo --kind symbols --limit 50 --timing
fcs bench tui /path/to/repo --query main --format json
```

建议：

- 以 `target-symbols=5000` 起步，再根据 `bench tui` 和 `shard-query` 调整。
- 对 C/C++ 大仓库优先检查 `compile_commands.json`，否则 clangd 语义能力会受限。
- 对 Rust 仓库先确认 `cargo check` 能通过或至少 rust-analyzer 能加载 workspace。
- 频繁变动的大仓库可用 `index daemon` 或 `service start` 保持缓存新鲜。

### 12.3 性能回归基线

```bash
fcs bench all . --query main --limit 50
fcs bench baseline .
fcs bench compare . --threshold-ms 10 --threshold-percent 25 --strict
```

建议在以下场景保存新基线：

- 大规模重建 index 或 shard 存储格式后。
- TUI source streaming 或 sidecar-backed 行为变更后。
- trace graph、semantic fallback、query engine 有性能相关改动后。

## 13. 排障

### 13.1 搜索结果缺失

排查顺序：

```bash
fcs ignore list
fcs files . -o --hidden -o --no-ignore --query target_name
fcs search target_name . -o --hidden -o --no-ignore
fcs index refresh .
fcs index query target_name . --kind symbols --timing
```

常见原因：

- `.ignore`、`.gitignore` 或全局配置忽略了目标路径。
- 搜索 pattern 是正则，特殊字符未转义。
- index 过期或 shard stale。
- 目标文件类型暂未被轻量 symbol extractor 覆盖。

### 13.2 LSP 无结果或超时

```bash
fcs lsp health . --file src/main.rs
fcs workspace doctor .
fcs diag src/main.rs
fcs graph semantic src/main.rs:42:5 --relation references --fallback index
```

处理建议：

- C/C++ 检查 `compile_commands.json` 是否存在且路径正确。
- Rust 检查 `Cargo.toml`、workspace 成员和 rust-analyzer 是否可启动。
- 增大 `[lsp].request_timeout_ms`。
- 使用 `--fallback index` 保留调查进度。

### 13.3 TUI 卡顿或结果延迟

```bash
fcs bench tui . --query main --format json
fcs index profile main . --kind symbols --format json --warn-ms 200
fcs index shard-status . --format json
```

处理建议：

- 对大仓库启用 shard。
- 收紧 ignore，排除生成目录。
- 使用 `group path` 或 `filter path` 降低可视结果压力。
- 使用 `service start` 或 `index daemon` 保持缓存热。

### 13.4 DAP 启动失败

```bash
fcs dap adapters
fcs dap doctor . --format json
fcs dap session-smoke target/debug/app -b src/main.rs:1
fcs dap adapter-session auto target/debug/app -b src/main.rs:1 --cwd . --format json
```

处理建议：

- Arch Linux 优先确认 `/usr/bin/lldb-dap` 是否随官方 `lldb` 包安装。
- AUR `codelldb` 安装失败时，不要阻塞；先用 `lldb-dap`。
- 检查二进制是否存在、是否带调试符号、工作目录是否正确。
- attach 需要有效 `--process-id`，且系统允许 ptrace。
- sandbox 或容器失败时，在普通本机 shell 复测。

### 13.5 Trace 或 Profile 数据异常

```bash
fcs trace verify --directory . --format json
fcs trace repair --directory . --format text
fcs trace compact --format json
fcs workspace doctor .
```

处理建议：

- dangling parent、重复 entry、引用路径移动后，先 `verify` 再 `repair`。
- 多条调查线混在一起时，用 `trace split` 按 tag 拆分。
- session 名称不统一时，用 `trace rename` 或 `trace merge`。

## 14. 发布和交付前检查

日常快速检查：

```bash
cargo test
scripts/smoke.sh fast
```

完整发布检查：

```bash
scripts/release-check.sh full
```

真实 DAP adapter smoke 是显式 opt-in：

```bash
FCS_REAL_DAP_SMOKE=1 scripts/smoke.sh fast
```

该检查需要 adapter 在 `PATH` 中，且当前环境允许 LLDB/GDB 对子进程执行 ptrace。受限环境失败时，应在普通本机 shell 中复测。

## 15. 常用命令速查

```bash
fcs tui . --mode symbols --query main
fcs search "TODO" . -o --hidden
fcs files . --query main
fcs symbol . --query parse
fcs preview src/main.rs:42:20
fcs workspace doctor .
fcs index refresh .
fcs index query main . --kind symbols --timing
fcs query "kind:function text:main" . --source all --profile
fcs def src/main.rs:42:5
fcs refs src/main.rs:42:5
fcs graph semantic src/main.rs:42:5 --relation outgoing --format mermaid --fallback index
fcs trace add src/main.rs:42 --session bug-42 --tag hot
fcs trace report bug-42 --format markdown
fcs debug command target/debug/app -b src/main.rs:42
fcs dap adapters
fcs dap adapter-session auto target/debug/app -b src/main.rs:42 --cwd . --format json
fcs bench all . --query main --limit 50
```
