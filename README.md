# fcs (Fuzzy Code Searcher) 🔍

`fcs` 是一个用 Rust 编写的高性能、交互式模糊代码搜索工具。它结合了 **Ripgrep** 的极速搜索能力、**Skim** (Fzf Rust 实现) 的交互式过滤、**Bat** 的语法高亮预览，并支持直接在 **Neovim / Vim** 等编辑器中跳转到匹配行。

---

## ✨ 核心特性

- 🚀 **极速搜索**：底层直接集成 `ripgrep` 系列库（`grep-searcher`, `grep-regex`, `ignore`），在数百万行代码中瞬间检索。
- 🎨 **实时预览**：采用 `bat` 库提供带有行号、网格和语法高亮的实时代码预览。
- ⌨️ **交互式过滤**：强大的模糊匹配过滤（支持 exact 模式、智能大小写、反向匹配等）。
- 🔌 **编辑器集成**：在交互界面按下回车键，可直接调用你配置的编辑器（如 `nvim`, `vim`）并**自动定位到匹配的行数**。
- ⚙️ **配置灵活**：支持全局配置文件 `fcs.toml`，可定制快捷键、配色方案、默认忽略路径等。
- 🙈 **智能忽略管理**：提供便捷的命令管理本地或全局缓存的 `.ignore` 规则。

---

## 🛠️ 安装与编译

### 1. 前置要求

在编译和使用 `fcs` 之前，请确保系统中已安装以下工具：

- **Rust 工具链** (Cargo & Rustc 1.70+)：用于编译项目。
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
```

### 4. 验证安装

运行以下命令验证 `fcs` 是否安装成功：

```bash
fcs --version
```

---

## 💡 使用方法

`fcs` 提供了多个子命令，以下是主要的用法说明。

### 1. 代码搜索 (`search`)

模糊搜索是 `fcs` 最核心的功能。它会在指定目录中通过 Regex 预筛选文件，然后进入交互式模糊过滤界面。

```bash
# 格式
fcs search <search_pattern> [directory] [options]

# 示例 1：在当前目录下搜索 "fn main"
fcs search "fn main"

# 示例 2：在指定目录 "/path/to/project" 下搜索 "struct Config"
fcs search "struct Config" /path/to/project

# 示例 3：传入额外的 Ripgrep 选项（例如不忽略任何文件、忽略大小写）
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

### 2. 忽略规则管理 (`ignore`)

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

### 3. 单文件预览 (`preview`)

直接预览指定文件特定行前后的上下文（不进入交互界面）：

```bash
# 格式
fcs preview <path>:<line>[:height]

# 示例：预览 src/main.rs 第 100 行，高度为 20 行的上下文
fcs preview src/main.rs:100:20
```

---

### 4. 命令行自动补全 (`complete`)

`fcs` 可以为各种 Shell 生成自动补全脚本：

```bash
# 生成 Zsh 补全脚本
fcs complete zsh > ~/.zsh/completion/_fcs

# 支持的 Shell 包括：bash, elvish, fish, powershell, zsh
```

---

## ⚙️ 配置文件说明

`fcs` 首次运行时会在用户的配置目录中自动生成 `fcs.toml` 默认配置文件：
- **Linux/macOS**: `~/.config/fcs/fcs.toml` 或 `$XDG_CONFIG_HOME/fcs/fcs.toml`

### 默认配置内容及说明

```toml
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
```
