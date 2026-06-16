# fcs (Fuzzy Code Searcher) 🔍

`fcs` 是一个用 Rust 编写的高性能、交互式代码搜索、追踪与调试辅助工具。它结合了 **Ripgrep** 的极速搜索能力、**Ratatui** 的常驻工作台、**Skim** 的 legacy picker、**Bat** 风格代码预览，并支持直接在 **Neovim / Vim** 等编辑器中跳转到匹配行。

---

## ✨ 核心特性

- 🚀 **极速搜索**：底层直接集成 `ripgrep` 系列库（`grep-searcher`, `grep-regex`, `ignore`），在数百万行代码中瞬间检索。
- 🖥️ **Ratatui 工作台**：`fcs tui` 提供常驻多面板界面，可循环搜索、切换 source、查看 preview、trace 和 debug 命令。
- 🎨 **实时预览**：采用 `bat` 库提供带有行号、网格和语法高亮的实时代码预览。
- ⌨️ **交互式过滤**：强大的模糊匹配过滤（支持 exact 模式、智能大小写、反向匹配等）。
- 🔌 **编辑器集成**：在交互界面按下回车键，可直接调用你配置的编辑器（如 `nvim`, `vim`）并**自动定位到匹配的行数**。
- 🧭 **文件与符号导航**：支持项目文件模糊查找，以及无需 clangd 的轻量级符号索引。
- 🧠 **语义导航**：支持通过 clangd 查询定义、引用和文件诊断。
- 🧵 **追踪会话**：自动记录打开过的位置，并支持手动书签、历史回放和调试器断点联动。
- 🔎 **统一查询与服务快照**：支持字段化查询 index/trace，并可用前台 service 生成 workspace 状态快照。
- 🧩 **声明式插件**：支持内置和项目级 TOML 插件，为项目提供可复用 commands/templates。
- ⚙️ **配置灵活**：支持全局配置文件 `fcs.toml`，可定制快捷键、配色方案、默认忽略路径等。
- 🙈 **智能忽略管理**：提供便捷的命令管理本地或全局缓存的 `.ignore` 规则。

---

## 🛠️ 安装与编译

### 1. 前置要求

在编译和使用 `fcs` 之前，请确保系统中已安装以下工具：

- **Rust 工具链** (Cargo & Rustc 1.91+)：用于编译项目。
- **Neovim** (或常用终端编辑器)：作为默认编辑器集成（若未安装，运行 `./install.sh` 会有提示，你可以通过环境变量 `EDITOR` 或 `VISUAL` 切换为其他编辑器，如 `vim`、`code`、`nano` 等）。

### 2. 编译步骤

在克隆项目到本地后，使用 Cargo 进行编译：

```bash
# 编译 Release 版本（推荐，优化性能）
cargo build --release
```

编译完成后，二进制文件会生成在 `./target/release/fcs`。

### 3. 安装

你可以将编译好的二进制文件直接安装到系统的 `PATH` 路径中：

```bash
# 方法 A：使用 Cargo 自动安装（推荐）
cargo install --path .

# 方法 B：手动复制到用户二进制目录
cp target/release/fcs ~/.local/bin/

# 方法 C：安装 release binary 和 man page 到本地 prefix
scripts/install-local.sh --prefix "$HOME/.local"

# 生成 man page
fcs man --stdout
fcs man --out-dir ~/.local/share/man/man1
```

### 4. 验证安装

运行以下命令验证 `fcs` 是否安装成功：

```bash
fcs --version
```

---

## 💡 使用方法

`fcs` 提供了多个子命令，以下是主要的用法说明。

### 1. Ratatui 工作台 (`tui`)

`fcs tui` 是新的主工作流入口。它不是一次性 picker，而是一个常驻代码追踪工作台：左侧切换 source，中间查看结果，右侧用语法高亮预览代码，下方显示带状态配色的 trace 和 debug 命令。

```bash
# 默认进入 search mode
fcs tui

# 进入文件查找模式并设置初始查询
fcs tui --mode files --query main

# 进入符号模式
fcs tui --mode symbols --query handle

# 指定 debug 面板使用的二进制
fcs tui --debug-binary target/debug/app

# 非交互回放 TUI 命令脚本，适合回归测试和录制追踪流程
fcs tui-script trace-loop.fcs . --mode symbols --query main --format json
```

`tui-script` 逐行执行 TUI 命令面板语法，也支持 `select <n>`、`move <delta>`、`wait <ms>` 和 `assert ...` 断言。常用断言包括 `assert results >= 1`、`assert status contains traced`、`assert trace-session bug-42`、`assert trace-view graph`、`assert layout debug`、`assert filter none`、`assert group path`、`assert pending none`；默认不会持久化 TUI state，确实需要写回 pins/breakpoints/navigation/session/layout/filter/group 时加 `--persist`。

#### TUI 快捷键

- `q` / `Esc` / `Ctrl-C`：退出并恢复终端。
- `/`：进入输入模式，修改 query；`Enter` 刷新结果。
- `Tab` / `Shift-Tab`：切换 source。
- `j` / `k` 或方向键：移动结果选择。
- `Enter` / `o`：打开当前结果并写入 trace。
- `gd`：对当前选中位置执行 LSP definition。
- `gr`：对当前选中位置执行 LSP references。
- `t` / `gt`：执行 LSP type definition。
- `i` / `gi`：执行 LSP implementation。
- `s`：查看当前文件 document symbols。
- `c` / `C`：查看 incoming / outgoing calls。
- `e`：查看当前文件 LSP diagnostics。
- `a`：把当前结果加入 trace bookmark。
- `b`：把当前结果加入断点列表。
- `B`：把 trace 中的位置批量加入断点列表。
- `D`：切换到 Debug source，显示断点和已保存 debug profile。
- `X`：显式启动 Debug source 中的 gdb 会话；在 Debug source 中选中 profile 时会运行该 profile。
- `x`：在 Debug source 中删除当前 profile 或断点。
- `F5` / `F6` / `F10` / `F11` / `Shift-F11` / `Ctrl-F5`：对 TUI DAP worker 执行 continue、pause、next、step in、step out、stop。
- `P`：锁定/解锁 preview；`PageUp` / `PageDown` 滚动 preview。
- `:`：打开命令面板，支持 `Tab` 补全和 `Up/Down` 历史；可输入 `source <mode>`、`query <text>`、`layout search/debug/trace/semantic/balanced`、`filter kind/path/text <value>`、`filter clear`、`group kind/path/none`、`status copy/health`、`preview lock/up/down/reset`、`def`、`refs`、`type`、`impl`、`symbols`、`diag`、`incoming`、`outgoing`、`hover`、`trace session <name>`、`trace view session/timeline/graph`、`trace current`、`trace sessions`、`trace semantic [relation]`、`trace breakpoint`、`trace dap-profile <name>`、`break sync`、`debug`、`run`、`open`、`refresh`、`delete`、`watch add/del/clear/refresh`、`var page/next/prev`、`eval <expr>`、`dap start <profile>`、`dap real <adapter-command>`、`dap sync`、`dap restart/terminate/disconnect`、`dap adapters`、`dap jump/open`、`quit`。
- `[` / `]`：在 TUI 内的导航栈中后退/前进。
- `?`：在状态栏显示快捷键提示。

> `fcs search/files/symbol` 等旧命令仍保留，适合脚本化或一次性选择；`fcs tui` 适合连续搜索、语义追踪、诊断和调试准备。

---

### 2. 代码搜索 (`search`)

模糊搜索是 `fcs` 最核心的功能。它会在指定目录中通过 Regex 预筛选文件，然后进入交互式模糊过滤界面。

```bash
# 格式
fcs search <search_pattern> [path ...] [options]

# 示例 1：在当前目录下搜索 "fn main"
fcs search "fn main"

# 示例 2：在指定目录 "/path/to/project" 下搜索 "struct Config"
fcs search "struct Config" /path/to/project

# 示例 3：在多个文件或目录中搜索 "main"
fcs search main install.sh README.md src

# 示例 4：传入额外的 Ripgrep 选项（例如不忽略任何文件、忽略大小写）
fcs search "TODO" -o --no-ignore -o -i
```

#### 支持的 Ripgrep 搜索选项 (`-o` / `--option`)
你可以通过多次传递 `-o` 或 `--option` 参数来微调搜索行为：
- `-i`, `--ignore-case`：忽略大小写。
- `-s`, `--case-sensitive`：区分大小写。
- `-S`, `--smart-case`：智能大小写（如果搜索词全为小写则忽略大小写，含大写则区分）。
- `-F`, `--fixed-strings`：将搜索词视为字面字符串而非正则表达式。
- `-w`, `--word-regexp`：仅匹配完整单词。
- `-x`, `--line-regexp`：仅匹配整行。
- `-v`, `--invert-match`：反向匹配（寻找不包含该模式的行）。
- `-L`, `--follow`：跟随符号链接（Symlinks）。
- `--no-ignore`：不读取 `.gitignore`、`.ignore` 等忽略文件。
- `-m`, `--max-count <NUM>`：限制每个文件的匹配行数。
- `-d`, `--max-depth <NUM>`：限制目录遍历的深度。

#### 交互界面快捷键 (Interactive Keybindings)
进入交互模糊过滤界面后，你可以使用以下默认快捷键进行操作：
- `Enter (回车)`：打开编辑器并跳转到选中的文件匹配行。
- `Ctrl + U` / `Ctrl + D`：列表向上/向下翻半页。
- `Alt + U` / `Alt + D`：右侧预览窗口向上/向下翻页。
- `Alt + J` / `Alt + K`：右侧预览窗口向上/向下滚动单行。
- `Ctrl + V`：显示/隐藏右侧预览窗口。
- `Ctrl + R`：清空当前输入的模糊搜索词。
- `Esc` / `Ctrl + C`：退出交互界面。

---

### 3. 文件查找 (`files`)

快速枚举项目文件并进入交互式模糊选择界面，回车后用配置的编辑器打开文件。

```bash
# 在当前目录下查找文件
fcs files

# 在指定目录下查找文件，并设置初始查询
fcs files /path/to/project -q main

# 传入文件遍历选项
fcs files -o --hidden -o --no-ignore
```

支持的文件遍历选项包括：
- `--hidden`：包含隐藏文件。
- `--no-ignore`：忽略 `.gitignore`、`.ignore` 和默认 ignore 配置。
- `-L`, `--follow`：跟随符号链接。
- `-d`, `--max-depth <NUM>`：限制目录遍历深度。

---

### 4. 符号查找 (`symbol`)

无需 clangd 即可进行轻量级符号查找。当前支持 Rust、C/C++、Python、JavaScript/TypeScript 中的常见函数、结构体、类、枚举、trait、宏等粗粒度符号。

`symbol` 内部使用 `grep-searcher` / `grep-regex` 先通过 ripgrep 搜索器高速筛选候选定义行，再提取符号名和符号类型。

```bash
# 在当前目录查找符号
fcs symbol

# 在指定目录查找符号，并设置初始查询
fcs symbol /path/to/project -q parse_config

# 限制扫描深度
fcs symbol -o --max-depth=3
```

> **说明**：`symbol` 当前是轻量 ripgrep + 正则索引，适合快速追踪；精确语义跳转请使用下方 clangd 命令。

---

### 5. 代码索引 (`index`)

`fcs index` 会把项目文件列表和轻量符号结果持久化到 workspace cache，适合后续做高性能浏览、状态检查和增量判断。

```bash
# 查看索引版本、缓存位置、tracked file 是否变更
fcs index status

# 构建索引；二次执行时只重新抽取新增/变更文件的 symbol，未变文件复用缓存
fcs index build

# 查看缓存中的符号或文件
fcs index list --kind symbols --limit 50
fcs index list --kind files --limit 50

# 查看缓存大小、语言分布、symbol kind 分布
fcs index stats

# 为大型 workspace 估算 shard 拆分建议
fcs index shards . --target-symbols 5000 --format json

# 写入 shard cache、检查新鲜度，并在 shard 上查询；stale 时自动回退主索引
fcs index shards . --target-symbols 5000 --write
fcs index shard-status .
fcs index shard-query parse_config . --kind symbols --limit 20

# 查询缓存索引，不重新扫描项目
fcs index query parse_config --kind symbols --limit 20 --timing --warn-ms 200

# 只检查主索引和 shard cache 健康状态，不做重建
fcs index verify . --format json

# 记录 status/stats/list/query/shard-query 各阶段延迟
fcs index profile parse_config . --kind symbols --limit 20 --format json --warn-ms 200

# 修复 stale/corrupt/missing index；--force 可强制重建
fcs index repair

# 仅在索引 missing/stale/corrupt/schema migrated 时重建
fcs index refresh

# 预热/压缩缓存
fcs index prewarm
fcs index compact --dry-run

# 前台轮询刷新索引；常驻模式省略 --max-cycles
fcs index daemon --interval-ms 2000 --max-cycles 1 --foreground
fcs index daemon-status

# 记录缓存操作延迟，并写入 workspace cache 的 latency-smoke.tsv
fcs index bench --limit 50 --query main
```

索引当前复用 `files` / `symbol` 的高速扫描路径，并记录 schema version、文件 language、文件大小、修改时间、内容 hash、每文件 symbol 数量、符号 language、range 和 parent 元数据。二次 `build` 仍会快速扫描文件清单以发现新增/删除，但 symbol 抽取优先依据内容 hash 判断变化，只作用于新增或变化文件，并在 build report 中输出新增/变化/复用数量和样本路径。`index shards --write` 会把大仓库按目录 bucket 写成 shard cache 和 manifest，`shard-query` 在 manifest stale/missing 时自动回退主索引。`index daemon` 是无额外依赖的轮询守护模式，每轮复用 `index refresh`，并在 workspace cache 写入 heartbeat，便于 `daemon-status` 检查最后一次刷新状态。

---

### 6. 统一查询、Service 与 Benchmark

`fcs query` 提供一个统一的字段化查询入口，可以同时查缓存 index 和 trace 历史。常用字段包括 `kind:`、`lang:` / `language:`、`path:`、`name:`、`text:`、`source:`、`status:`、`priority:`、`session:` 和 `tag:`。

```bash
# 从 index + trace 中查询
fcs query "kind:function lang:rust text:parse" . --source all --format json

# 只查 index，适合脚本化筛选符号
fcs query "path:src name:main" . --source index --limit 20

# 只查 trace，适合复盘调查状态
fcs query "session:bug-42 status:open tag:hot" . --source trace

# 解释字段解析、source 选择和支持字段，不执行查询
fcs query "source:index kind:function name:main" . --source all --explain

# 支持分组、OR 和 NOT，用于更精确的代码追踪过滤
fcs query "kind:function (name:parse or name:init) not path:target" . --source all --explain

# 输出慢查询观测信息
fcs query "source:index kind:function text:main" . --source all --timing --warn-ms 200

# 输出聚合 profile 报告，便于定位 source 选择、过滤器和结果分布
fcs query "source:index kind:function text:main" . --source all --profile --format json --warn-ms 200

# 切换匹配模式、使用内置 macro，并在结果 detail 中输出 score
fcs query "name:parse_.*" . --source index --mode regex --macro functions --score-explain
fcs query "kind:function name:parse_config" . --source index --mode exact

# 保存、复用、列出和删除当前 workspace 的常用查询
fcs query "kind:function name:parse_config" . --source index --mode exact --save parse-config
fcs query --use parse-config --source index --mode exact
fcs query --list-saved
fcs query --delete-saved parse-config

# 只查 LSP workspace/symbol；LSP 不可用时回退到本地 index 结果
fcs query "name:parse_config" . --source semantic

# 融合 index + trace + LSP；LSP 不可用时仍返回本地结果和状态项
fcs query "kind:function text:main" . --source auto
```

`fcs query --explain` 会打印执行计划、字段过滤器和候选数据源，适合调试复杂表达式。`--profile` 会执行查询并输出 source/kind 分布、实际 execution plan、filters、macro 和耗时，适合给慢查询或复杂过滤做回归基线。`--mode fuzzy|exact|regex` 可在快速模糊匹配、严格 token 匹配和正则匹配之间切换；`--macro functions|tests|todo|rust|c|debug` 用于把常见过滤器拼进表达式。`--source semantic` 会优先使用 LSP workspace/symbol；当 LSP 配置缺失或 adapter 查询失败时，会返回带 `fallback:index:*` 来源前缀的本地 index 结果，避免追踪链路因为语义服务不可用而完全中断。

`fcs service` 是无额外依赖的前台轮询服务，用于把 index、LSP provider 健康、trace、plugin 诊断和当前 workspace profile 汇总成 workspace cache 中的快照文件。它不会后台 fork；需要常驻时建议由 shell、systemd、tmux 或任务编排器托管。

```bash
# 跑一轮刷新并写入 heartbeat/snapshot
fcs service start . --interval-ms 2000 --max-cycles 1 --foreground

# 查看最近 heartbeat 和 snapshot 摘要
fcs service status .
fcs service snapshot . --format json

# 复用统一查询引擎
fcs service query "kind:function text:main" . --source index --format json
fcs service query "source:index kind:function text:main" . --source all --explain

# 请求常驻前台 service 在下一轮停止
fcs service stop .
```

`fcs bench` 用于给搜索、索引、TUI source、trace store 和 preview 读取建立本地延迟基线；`bench all` 会把 `benchmark-report.json` 写入 workspace cache。

```bash
fcs bench search main . --format json --warn-ms 200
fcs bench index . --limit 50 --query main --warn-ms 200
fcs bench tui . --query main --format json
fcs bench trace --format json
fcs bench preview src/main.rs:20 --warn-ms 20
fcs bench all . --query main --limit 50
fcs bench baseline .
fcs bench compare . --format json --threshold-ms 10 --threshold-percent 25 --strict
```

`bench baseline` 会把最近一次 `bench all` 的报告保存为 `benchmark-baseline.json`；`bench compare` 对比当前报告与基线，适合 release smoke 或本地性能回归门禁。每次写入 workspace benchmark report 时也会追加 `benchmark-history.json`，text 输出会在有历史时显示 `trend:`，慢项会显示 `explain:` 建议。

workspace profile 和配置诊断适合 monorepo 或多根项目：

```bash
fcs workspace profile save core . --description "core workspace" --index-root src
fcs workspace profile list
fcs workspace profile use core
fcs workspace profile current
fcs workspace plan
fcs workspace workflows . --format text
fcs workspace config-doctor .
fcs workspace config-schema --format toml
fcs workspace config-migrate . --dry-run
```

---

### 7. LSP 语义导航

语义导航会按文件类型选择 LSP provider：Rust 使用 `rust-analyzer`，C/C++ 使用 `clangd`。对于 C/C++ 项目，建议项目根目录存在 `compile_commands.json` 或 `compile_flags.txt`；Rust 项目建议保留 `Cargo.toml` 并确保 `rust-analyzer` 在 `PATH` 中。

```bash
# 检查 workspace 语义导航就绪状态
fcs workspace status

# 输出项目识别结果和可执行建议
fcs workspace advise

# 输出 TUI Activity 面板使用的非阻塞启动计划
fcs workspace plan

# 输出面向 crash、symbol、diagnostic、trace->DAP 的诊断 workflow 模板
fcs workspace workflows

# 只查看项目自动识别结果，或执行更完整健康检查
fcs workspace detect
fcs workspace doctor
fcs workspace doctor-bundle . --format json --out /tmp/fcs-doctor.json

# 初始化 fcs 的非侵入式 workspace 缓存元数据
fcs workspace init

# 跳转定义
fcs def src/main.c:42:5
fcs def src/lib.rs:42:5

# 查找引用
fcs refs src/main.c:42:5

# 查看单文件诊断
fcs diag src/main.c

# 查看 hover 文本和 workspace symbol
fcs hover src/main.c:42:5
fcs workspace-symbols parse_config --limit 50

# 更深层的 LSP 调试辅助
fcs lsp highlights src/main.c:42:5
fcs lsp refs src/main.c:42:5
fcs lsp rename src/main.c:42:5 new_name
fcs lsp rename src/main.c:42:5 new_name --apply --dry-run
fcs lsp code-actions src/main.c:42:5
fcs lsp code-actions src/main.c:42:5 --format json
fcs lsp code-actions src/main.c:42:5 --apply 1 --dry-run
fcs lsp organize-imports src/main.c --apply --dry-run
fcs lsp outline src/main.c --format tree
fcs lsp breadcrumbs src/main.c:42:5
fcs lsp semantic-tokens src/main.c --line 42
fcs lsp call-tree src/main.c:42:5

# 检查当前 workspace 或指定文件使用的 LSP provider
fcs lsp health
fcs lsp health --file src/main.c
```

`workspace doctor-bundle` 会把 startup plan、config diagnostics、index status/stats/shards、service snapshot、DAP profiles/adapters/templates、workflow 模板和 saved queries 打包成 text/json，适合提交 issue 或做 release 前环境快照。`workspace workflows` 现在包含 `search-to-debug-loop`，把 query、trace、`graph semantic --fallback index`、DAP profile 和 TUI debug 面板串成一个可重复的追踪循环。

### 8. 语义图与导入图 (`graph`)

`fcs graph` 用来把追踪过程中的关系导出成可读边列表、JSON、Mermaid 或 Graphviz DOT。`semantic` 子命令复用 LSP provider，适合定义、引用、类型定义、实现和调用关系；`imports` 子命令使用轻量文件扫描，适合快速观察模块依赖。

```bash
# clangd-backed semantic graph
fcs graph semantic src/main.c:42:5 --relation outgoing --format text
fcs graph semantic src/main.c:42:5 --relation references --format json
fcs graph semantic src/main.c:42:5 --relation outgoing --format dot --fanout 20
fcs graph semantic src/main.c:42:5 --relation outgoing --format json --fallback index --cache --refresh-cache
fcs graph semantic src/main.c:42:5 --relation outgoing --format json --fallback index --cache

# Record the same semantic relation into a trace session
fcs trace semantic src/main.c:42:5 --relation outgoing --session bug-42 --fallback index --cache

# Lightweight import/use/mod graph
fcs graph imports --limit 100 --format text
fcs graph imports --format json
fcs graph imports --limit 100 --depth 2 --fanout 8 --exclude target --format mermaid
fcs graph imports --limit 100 --format dot

# Offline module/call graph without requiring an LSP server
fcs graph modules --limit 100 --depth 2 --format dot
fcs graph calls --limit 100 --fanout 8 --format json
```

支持的 semantic relation：`references` / `definition` / `type` / `implementation` / `incoming` / `outgoing`。
支持的 graph format：`text` / `json` / `mermaid` / `dot`。`--fanout` 限制每个 source 的最大出边数，`--exclude` 可重复传入并按 source/target/kind/detail 的子串过滤；`imports/modules --depth` 会在解析到本地模块文件时做有限深度扩展。`graph semantic --cache` 会把同一 root/location/relation/depth/fanout/filter/fallback 的结果缓存到 workspace cache，`--refresh-cache` 强制刷新后再写入，适合把昂贵的语义追踪步骤纳入 smoke 或诊断脚本。`trace semantic` 复用同一条语义查询链路，但会把源点记录为 `semantic-root`，把返回的目标记录为 `semantic:<relation>` 子节点，便于后续 `trace graph/report/insights` 或 `debug from-trace` 继续使用。`calls` 是离线近似调用图，适合快速追踪热点路径，精确语义仍建议使用 LSP-backed `graph semantic`。

---

### 9. 追踪、历史与调试器联动

通过 `fcs` 打开的搜索、文件、符号、引用结果会自动写入 trace。也可以手动添加书签。

```bash
# 手动添加书签，可带 session/parent/branch/tag
fcs trace add src/main.c:42 -l "init path" --session bug-42 --branch main --tag hot

# 查看或交互打开 trace
fcs trace list
fcs trace list --session bug-42 --tag hot --status open
fcs trace open

# 导出调查报告、结构化报告或 parent/child graph
fcs trace export
fcs trace export --format json
fcs trace graph

# 管理调查 session
fcs trace sessions
fcs trace use bug-42
fcs trace current
fcs trace archive bug-42
fcs trace unarchive bug-42
fcs trace report bug-42 --format markdown
fcs trace report bug-42 --format json
fcs trace timeline bug-42 --format json
fcs trace replay bug-42 --format markdown
fcs trace replay-plan bug-42 --program target/debug/app --name bug-42-dap --format json
fcs trace structured bug-42 --format json
fcs trace insights bug-42 --directory . --format markdown
fcs trace diff bug-42 bug-42-next --format json
fcs trace diff bug-42 bug-42-next --format json --filter semantic
fcs trace rename bug-42-old bug-42
fcs trace merge bug-42-spike bug-42
fcs trace split bug-42 bug-42-hot --tag hot
fcs trace verify --directory . --format json --strict
fcs trace repair --directory . --format text
fcs trace compact --format json

# 语义追踪支持单点、targets 文件和 query 批量输入；graph 可导出 text/json/mermaid/dot
fcs trace semantic src/main.c:42 --relation outgoing --session bug-42 --fallback index
fcs trace semantic --targets-file targets.txt --relation references --session bug-42
fcs trace semantic --from-query "kind:function name:init" --query-source index --query-limit 10 --directory .
fcs trace graph --format mermaid --session bug-42 --tag hot --collapse-threshold 8

# 查看查询历史
fcs history list

# 基于显式断点生成 gdb 命令
fcs debug command target/debug/app -b src/main.c:42 --cwd . --env RUST_LOG=debug

# 使用最近 trace 位置作为断点
fcs debug last target/debug/app

# 使用 trace session 的所有行位置生成调试 profile
fcs debug from-trace bug-42 target/debug/app --name bug-42-debug --cwd . --env RUST_LOG=debug -- --config dev.toml

# 显式启动调试器
fcs debug command target/debug/app -b src/main.c:42 --run
```

`debug` 默认只打印命令，不会擅自进入交互式调试器。加 `--run` 后才启动 `gdb` 或 `lldb`。

`trace graph` 可按 `--session`、`--tag`、`--kind`、`--status`、`--priority` 和 `--relation` 过滤，并可用 `--collapse-threshold` 把大批同 session/kind/path 的节点折叠成 summary。`trace insights` 会在普通 session report 之上汇总 kind/status/priority、热点文件、debug/DAP 事件和未关闭条目；提供 `--directory` 且存在 index 时，还会把 trace 位置关联到最近的索引符号。`trace verify/repair/compact` 用于发布前或长时间使用后的 trace store 健康检查和去重维护。

---

### 10. DAP 请求与 Profile (`dap`)

`fcs dap` 面向 VS Code、nvim-dap 等 Debug Adapter Protocol 前端，生成基础 `launch` 请求和 `setBreakpoints` bundle，也可以把 launch profile 保存在 workspace cache 中。

```bash
# 只打印 launch request
fcs dap launch target/debug/app -- --config dev.toml

# 打印 attach request；当前 attach 模板需要 process id
fcs dap launch target/debug/app --request attach --process-id 12345

# 打印 setBreakpoints + launch 的请求数组
fcs dap launch target/debug/app -b src/main.c:42 --bundle -- --config dev.toml
fcs dap launch target/debug/app -b src/main.c:42 --break-condition "argc > 1" --break-hit 3 --break-log "main hit" --bundle

# 保存、列出、复用 profile
fcs dap save-profile smoke target/debug/app -b src/main.c:42 --cwd . --env RUST_LOG=debug -- --config dev.toml
fcs dap profiles
fcs dap request-profile smoke --bundle
fcs dap transcript smoke --format json

# 使用 trace session 生成 DAP profile
fcs dap from-trace bug-42 target/debug/app --name bug-42-dap --cwd . --env RUST_LOG=debug -- --config dev.toml

# 使用 mock adapter 做非交互 DAP 会话 smoke
fcs dap session-smoke target/debug/app -b src/main.c:42 -- --config dev.toml

# 查看本机可用的 DAP adapter 候选；不会自动安装
fcs dap adapters

# 检查已保存 DAP profile、断点路径、cwd/program 和本机 adapter 可用性
fcs dap doctor . --format json
fcs dap doctor . --name smoke --format text

# 查看内置 adapter 模板和声明的能力标签
fcs dap templates

# 使用真实 DAP adapter 进程做 initialize/launch/configurationDone 会话
fcs dap adapter-session /path/to/adapter target/debug/app -b src/main.c:42 --cwd . -- --config dev.toml
fcs dap adapter-session auto target/debug/app -b src/main.c:42 --cwd . -- --config dev.toml
```

`dap launch/save-profile/session-smoke/adapter-session` 支持 `--break-condition`、`--break-hit` 和 `--break-log`，一个值会套用到全部断点，多个值会按断点序号对应。`dap launch/save-profile/request-profile/transcript` 仍适合脚本化生成请求；`--request attach --process-id <pid>` 可生成或执行 attach 请求。`dap doctor` 不会启动 adapter，会检查已保存 profile 的 request/processId、program、cwd、断点路径/行号，以及本机可发现 adapter，适合在 TUI 调试前先做环境诊断。`dap templates` 会展示每个内置 adapter 的 launch/attach 字段 schema、注意事项和参数预览，但不会改变真实 DAP request 的序列化。`dap session-smoke` 使用内置 mock adapter 验证 `initialize`、`setBreakpoints`、`launch/attach`、`configurationDone`、线程/栈帧/变量查询和 step/continue 请求链路。`dap adapter-session` 会启动真实 adapter 进程，当前覆盖非交互 launch/attach 编排；`auto` 会从 `lldb-dap`、`codelldb`、`OpenDebugAD7` 等常见命令中选择可用候选，并展示 capability 标签。TUI 的命令面板支持 `dap smoke`、`dap start <profile>`、`dap real <adapter-command>`、`dap sync`、`dap next/continue/pause/step-in/step-out/restart/terminate/disconnect`、`dap thread <id>`、`dap frame <index>`、`var expand <ref>`、`var page <start> <count>` 和 `dap jump/open`；Debug 面板会分区显示 session state、selected thread/frame、variable page/ref、last request/error、capabilities、stack、variables、watches、verified breakpoints、events，并把停止位置、栈顶和变量摘要写入 trace。

---

### 11. 忽略规则管理 (`ignore`)

你可以方便地初始化和修改本地或项目的忽略规则。

```bash
# 初始化当前目录（或指定目录）的 .ignore 文件并写入默认过滤配置
fcs ignore init

# 向忽略规则中添加模式
fcs ignore add "*.log" "target/"

# 从忽略规则中移除模式
fcs ignore remove "*.log"

# 列出当前生效的忽略模式
fcs ignore list
```

> **💡 说明**：
> 如果当前目录存在本地的 `.ignore` 文件，`fcs` 会直接对其进行读写；如果不存在，则会自动管理位于用户缓存目录中的特定 ignore 规则文件（路径通常为 `~/.cache/fcs/[project_name]-[hash].ignore`），避免污染项目工作区。

---

### 12. 单文件预览 (`preview`)

直接预览指定文件特定行前后的上下文（不进入交互界面）：

```bash
# 格式
fcs preview <path>:<line>[:height]

# 示例：预览 src/main.rs 第 100 行，高度为 20 行的上下文
fcs preview src/main.rs:100:20
```

---

### 13. 命令行自动补全 (`complete`)

`fcs` 可以为各种 Shell 生成自动补全脚本：

```bash
# 生成 Zsh 补全脚本
fcs complete zsh > ~/.zsh/completion/_fcs

# 支持的 Shell 包括：bash, elvish, fish, powershell, zsh
```

---

## 🧩 高级工作流

### 异步 TUI 搜索

`fcs tui` 中的 `Search / Files / Symbols` source 使用后台 worker。连续修改 query 时，worker 会丢弃队列中的旧请求，只执行最新请求；UI 主线程保持响应。`gd/gr/e/W/h/c` 等 clangd 请求也通过后台 LSP worker 执行，并复用同一个 clangd 进程。命令面板里的 `dap smoke` 通过 DAP worker 执行，停止位置会写入 trace 的 `debug-stop` 记录。

### 项目级配置

可以在项目根目录生成 `.fcs.toml`：

```bash
fcs workspace config
fcs workspace config --force
```

项目级配置会覆盖 TUI 中的 clangd 命令、默认 debug binary，并扩展默认 ignore：

```toml
clangd_command = "clangd"
default_debug_binary = "target/debug/app"
search_ignore = [".git/", "target/", "node_modules/"]

[[actions]]
name = "test-symbol"
description = "Run tests related to the current symbol"
command = "cargo"
args = ["test", "{symbol}", "--manifest-path", "{workspace}/Cargo.toml"]
cwd = "{workspace}"
```

自定义 action 可以在全局 `fcs.toml` 或项目 `.fcs.toml` 中配置；项目 action 会覆盖同名全局 action。支持变量：
`{workspace}`、`{file}`、`{line}`、`{symbol}`。

```bash
fcs actions list
fcs actions list /path/to/project
fcs actions templates
fcs actions init rust-cargo-test --dry-run
fcs actions doctor
fcs actions run test-symbol --file src/lib.rs --line 42 --symbol parse_config --dry-run
fcs actions run test-symbol --directory /path/to/project -- --exact
```

`actions templates` 提供内置 `rust-cargo-test`、`cpp-cmake-test`、`make-test`、`pytest`、`npm-test` 等模板；`actions init <template>` 可以生成项目 `.fcs.toml`，`--dry-run` 用于预览，`--force` 才覆盖已有配置。`actions doctor` 会检查模板变量和 action cwd 展开结果。

### 插件 Manifest

`fcs plugin` 会发现内置插件、`$XDG_CONFIG_HOME/fcs/plugins/*.toml` 和 `<workspace>/.fcs/plugins/*.toml`。插件是声明式 TOML，不加载动态库；它可以提供可运行 commands 和可初始化到 `.fcs.toml` 的 action templates。

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

插件 commands/templates 支持和 actions 一致的 `{workspace}`、`{file}`、`{line}`、`{symbol}` 变量；commands 还支持 `env = { KEY = "VALUE" }`、`[[commands.pre]]`、`[[commands.post]]`，以及 `{env.NAME}` 和 `--var KEY=VALUE` 对应的 `{var.KEY}`。

### Debug Profile

保存、列出和运行调试 profile：

```bash
fcs debug save-profile smoke target/debug/app -b src/main.rs:1 -- --help
fcs debug profiles
fcs debug run-profile smoke
fcs debug run-profile smoke --run
```

profile 保存在 workspace cache 中，包含 debugger、binary、args 和断点组。
profile 支持 `cwd`、`env`、删除，以及按 1-based index 启用/禁用断点：

```bash
fcs debug save-profile smoke target/debug/app -b src/main.rs:1 --cwd . --env RUST_LOG=debug -- --help
fcs debug disable-breakpoint smoke 1
fcs debug enable-breakpoint smoke 1
fcs debug delete-profile smoke
```

### Trace Report

Trace 支持 workspace-scoped 记录、Markdown/JSON 导出和 parent/child graph 输出：

```bash
fcs trace add src/main.rs:10 --session bug-42 --tag regression
fcs trace note latest "checked failing path"
fcs trace status latest open
fcs trace priority latest high
fcs trace semantic src/main.rs:10:1 --relation references --session bug-42 --fallback index
fcs trace sessions
fcs trace report bug-42
fcs trace timeline bug-42 --format json
fcs trace structured bug-42 --format markdown
fcs trace diff bug-42 bug-42-fix --format json
fcs trace export
fcs trace export --directory .
fcs trace export --format json
fcs trace graph --directory .
```

`trace list` 支持 `--session`、`--tag`、`--kind`、`--status`、`--priority` 过滤。`trace note/status/priority` 接受 trace id，也接受 `latest`；传入 `-` 可清空字段。session report 会汇总 status/priority，timeline 按时间输出排查过程，structured 单独输出 hypotheses/evidence/conclusions/open questions，diff 用于比较两条调查分支。
TUI 内通过 `a` 加书签，通过 `B` 把当前 workspace trace 批量转换为断点。

### 当前限制

- 搜索 worker 会取消队列中的旧请求，并对正在执行的 ripgrep-library 搜索做协作式取消；极短搜索可能在取消信号到达前自然结束。
- LSP worker 已经避免 UI 主线程阻塞，但 clangd 本身超时仍取决于 `request_timeout_ms`。
- TUI preview 读取目标行附近窗口并做缓存，内置 Rust/C/C++/Python/Shell/TOML/JSON/Markdown 等常见语法的轻量高亮；脚本化预览仍可使用 bat 风格的 `fcs preview`。

### 内部结构

TUI 已拆成小模块：
- `tui/actions.rs`：按键到 `AppAction` 的映射。
- `tui/sources.rs`：`SourceMode`、source worker、Search/Files/Symbols 的 `SourceProvider`。
- `tui/lsp_worker.rs`：长生命周期 clangd worker。
- `tui/render.rs`：ratatui 渲染层。
- `tui/highlight.rs`：TUI preview 语法高亮和结果/trace/debug/activity 面板配色。
- `tui/preview_cache.rs`：preview 窗口缓存。
- `tui/state.rs`：workspace-scoped TUI state，保存上次 mode/query、pins、jump stack、breakpoints、preview lock 和命令历史。

---

## ⚙️ 配置文件说明

`fcs` 首次运行时会在用户的配置目录中自动生成 `fcs.toml` 默认配置文件：
- **Linux/macOS**: `~/.config/fcs/fcs.toml` 或 `$XDG_CONFIG_HOME/fcs/fcs.toml`

全局配置文件带有顶层 `schema_version`。当前支持版本为 `1`；旧配置缺失该字段时仍会按兼容默认值读取，未来版本配置会给出明确错误，提示升级 `fcs` 或恢复到当前工具支持的配置版本。

### 默认配置内容及说明

```toml
schema_version = 1

[search]
# 默认追加的 ripgrep 搜索参数（例如：["-S"] 开启默认智能大小写）
rg_options = []

# 默认全局忽略的文件或目录列表
ignore = [
    ".git/",
    "target/",
    "node_modules/",
    "*.tmp",
    "*.log"
]

[skim]
# 交互界面按键绑定设置
binds = [
    "ctrl-u:half-page-up",
    "ctrl-d:half-page-down",
    "ctrl-r:kill-line",
    "ctrl-v:toggle-preview",
    "alt-u:preview-page-up",
    "alt-d:preview-page-down",
    "alt-j:preview-down",
    "alt-k:preview-up"
]
height = "100%"          # 交互界面的高度占比
min_height = "20"        # 交互界面的最小高度
exact = true             # 是否默认开启精确匹配模式（非模糊）
tac = true               # 结果是否反向排列（最新/匹配度最高的在最上方）
cycle = true             # 列表滚动是否循环
preview_window = "right:59%" # 预览窗口位置与大小比例

# 交互界面的配色方案（ANSI 颜色配置）
color = "fg:-1,bg:-1,hl:33,fg+:254,bg+:235,hl+:33,info:136,prompt:136,pointer:230,marker:230,spinner:136"

[editor]
# 可选。为空时依次读取 VISUAL、EDITOR，最后回退到 nvim。
# nvim/vim 会使用 "+line" 跳转；code/codium 会使用 "-g path:line[:column]" 跳转。
command = "nvim"

[lsp]
# clangd 启动命令，可带参数。
clangd_command = "clangd"

# LSP 请求超时时间。
request_timeout_ms = 3000

[tui.keymap]
# TUI 中少数高频入口可以改键；复杂语义动作仍保留内置快捷键和命令面板。
command_palette = ":"
query = "/"
open = "o"
refresh = "r"
trace = "a"
breakpoint = "b"
debug = "D"

[tui.theme]
# TUI 颜色和语法高亮开关；低色终端可开启 low_color。
name = "default"
color = true
syntax_highlight = true
low_color = false

[[actions]]
# 可选。自定义命令动作，项目 `.fcs.toml` 中同名 action 会覆盖全局 action。
name = "test-symbol"
description = "Run tests near the current symbol"
command = "cargo"
args = ["test", "{symbol}"]
cwd = "{workspace}"
```

---

## ✅ 验证建议

### 自动化测试

基础回归：

```bash
cargo test
```

当前单测重点覆盖：
- ripgrep 搜索选项、文件枚举、轻量符号提取。
- trace 元数据兼容、workspace 过滤、Markdown/JSON/graph 导出和 TOML 持久化。
- debug profile 的 cwd/env、禁用断点、同名覆盖、删除和 TOML 持久化。
- workspace root 解析、项目级 `.fcs.toml` 写入/读取、workspace cache 路径稳定性。
- TUI 的 source/action/selection/query/history 纯状态逻辑、preview cache，以及异步搜索结果的 stale response 过滤。

本仓库的 agent 工作流要求 shell 命令前缀 `rtk`，等价验证命令为：

```bash
rtk cargo test
```

发布前推荐执行完整 smoke 脚本。脚本会覆盖单测、clippy、CLI help、workspace profile/config doctor/schema/doctor-bundle、query mode/macro/saved-query、service query mode、bench、trace export/graph/session edit/timeline/diff/structured/insights/replay-plan、semantic graph index fallback、index query/repair/bench/daemon/shards write/status/query、project action templates、plugin schema/plan、debug profile、DAP mock launch/attach 和 adapter/template schema 发现流程；所有命令都通过 `rtk` 执行：

```bash
rtk scripts/smoke.sh
rtk scripts/smoke.sh fast
rtk scripts/smoke.sh release
```

`scripts/smoke.sh` 默认执行 `full`；`fast` 跳过最重的 test/clippy 但仍跑 CLI/TUI smoke；`release` 在 full 基础上额外执行 release build。发布门禁分为快速和完整两级。日常改动推荐先跑 fast：

```bash
rtk scripts/release-check.sh fast
```

准备发版或交付候选版本时运行 full；不传参数时默认也是 full：

```bash
rtk scripts/release-check.sh full
```

发布流程的逐项核查见 `RELEASE_CHECKLIST.md`，用户可见变更记录见 `CHANGELOG.md`。

### 手工 smoke

CLI smoke：

```bash
rtk cargo clippy -- -D warnings
rtk cargo run -- --help
rtk cargo run -- workspace status
rtk cargo run -- debug command target/debug/fcs -b src/main.rs:1
```

TUI smoke 需要真实终端，适合本地手工执行：

```bash
rtk cargo run -- tui --help
rtk cargo run -- tui --mode files --query main
rtk cargo run -- tui --mode search --query "fn "
rtk cargo run -- tui --mode symbols --query handle
```

TUI 验收重点：退出后终端恢复正常；`/` 可循环搜索；preview 随选择刷新；`gd/gr/t/i/s/c/C/e` 失败时只在状态栏提示；`a/b/B/D/X` 能更新 trace/debug 面板。
