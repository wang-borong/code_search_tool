use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "fcs", author, version, about = "Fuzzy code search tool", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage ignore patterns
    Ignore {
        #[command(subcommand)]
        action: IgnoreAction,

        /// Target directory (default: current directory)
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Preview a file at a specific line
    Preview {
        /// Format: "path:line" or "path:line:height"
        target: String,
    },

    /// Open the ratatui code tracing workspace
    Tui {
        /// Target directory
        directory: Option<String>,

        /// Initial source mode: search, files, symbols, refs, diag, trace, pinned, debug
        #[arg(short, long)]
        mode: Option<String>,

        /// Initial query
        #[arg(short, long)]
        query: Option<String>,

        /// Binary used by the TUI debug pane
        #[arg(long)]
        debug_binary: Option<String>,
    },

    /// Replay a TUI command script without opening an interactive terminal
    TuiScript {
        /// Script file; blank lines and lines starting with # are ignored
        script: String,

        /// Target directory
        directory: Option<String>,

        /// Initial source mode: search, files, symbols, refs, diag, trace, pinned, debug
        #[arg(short, long)]
        mode: Option<String>,

        /// Initial query
        #[arg(short, long)]
        query: Option<String>,

        /// Binary used by the TUI debug pane
        #[arg(long)]
        debug_binary: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Milliseconds to wait for source/LSP/DAP workers after each script command
        #[arg(long, default_value_t = 2000)]
        step_timeout_ms: u64,

        /// Persist TUI state after the script completes
        #[arg(long)]
        persist: bool,
    },

    /// Inspect or initialize workspace metadata
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },

    /// Run or inspect the unified fcs background service snapshot
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Build and inspect the workspace code index cache
    Index {
        #[command(subcommand)]
        action: IndexAction,
    },

    /// Query index and trace data with field filters
    Query {
        /// Query expression, e.g. 'kind:function lang:rust path:src text:main'
        expression: Option<String>,

        /// Target workspace directory
        directory: Option<String>,

        /// Query source: index, trace, or all
        #[arg(short, long)]
        source: Option<String>,

        /// Query matching mode: fuzzy, exact, or regex
        #[arg(long)]
        mode: Option<String>,

        /// Apply a built-in query macro, e.g. functions, tests, todo
        #[arg(long = "macro")]
        macros: Vec<String>,

        /// Maximum entries to print
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Print the parsed query plan instead of running it
        #[arg(long)]
        explain: bool,

        /// Print query latency in milliseconds
        #[arg(long)]
        timing: bool,

        /// Print a warning when query latency exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Save the expression as a named workspace query before running it
        #[arg(long)]
        save: Option<String>,

        /// Run a saved workspace query by name
        #[arg(long = "use")]
        use_query: Option<String>,

        /// List saved workspace queries
        #[arg(long)]
        list_saved: bool,

        /// Delete a saved workspace query by name
        #[arg(long)]
        delete_saved: Option<String>,

        /// Append score and mode details to each match
        #[arg(long)]
        score_explain: bool,

        /// Print an aggregate profile report instead of raw matches
        #[arg(long)]
        profile: bool,
    },

    /// Measure search, index, trace, and preview latency
    Bench {
        #[command(subcommand)]
        action: BenchAction,
    },

    /// Build semantic, call, and import graph views
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },

    /// List and run configured project actions
    Actions {
        #[command(subcommand)]
        action: ProjectAction,
    },

    /// Discover and run declarative fcs plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Jump to LSP definition for path:line[:column]
    Def {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP references for path:line[:column]
    Refs {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Jump to LSP type definition for path:line[:column]
    TypeDef {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP implementations for path:line[:column]
    Implementation {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP document symbols for a file
    DocSymbols {
        /// Source file to inspect
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP incoming calls for path:line[:column]
    Incoming {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP outgoing calls for path:line[:column]
    Outgoing {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP diagnostics for a file
    Diag {
        /// Source file to diagnose
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP hover text for path:line[:column]
    Hover {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show LSP workspace symbols for a query
    WorkspaceSymbols {
        /// Symbol query
        query: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Maximum entries to print or pick
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
    },

    /// Inspect LSP provider health
    Lsp {
        #[command(subcommand)]
        action: LspAction,
    },

    /// Manage trace history and bookmarks
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },

    /// Show or clear query history
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Generate or launch debugger sessions from trace locations
    Debug {
        #[command(subcommand)]
        action: DebugAction,
    },

    /// Generate and store basic Debug Adapter Protocol launch requests
    Dap {
        #[command(subcommand)]
        action: DapAction,
    },

    /// Fuzzy-find files in a project
    Files {
        /// Target directory to search in
        directory: Option<String>,

        /// Initial skim query
        #[arg(short, long)]
        query: Option<String>,

        /// File search options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Fuzzy-find coarse symbols without requiring an LSP server
    Symbol {
        /// Target directory to search in
        directory: Option<String>,

        /// Initial skim query
        #[arg(short, long)]
        query: Option<String>,

        /// Symbol search file options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Search patterns in files
    Search {
        /// Search pattern (regex)
        pattern: String,

        /// Target directory to search in
        directory: Option<String>,

        /// Ripgrep-compatible search options (e.g. -i/--ignore-case or --no-ignore)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Generate shell completion script
    Complete {
        /// Target shell (bash, elvish, fish, powershell, zsh)
        shell: clap_complete::Shell,
    },

    /// Generate a simple man page
    Man {
        /// Print the generated man page to stdout
        #[arg(long)]
        stdout: bool,

        /// Directory where fcs.1 should be written
        #[arg(long)]
        out_dir: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TraceAction {
    /// Add a location to trace history
    Add {
        /// Format: "path", "path:line", or "path:line:column"
        target: String,

        /// Optional label
        #[arg(short, long)]
        label: Option<String>,

        /// Entry kind
        #[arg(short, long, default_value = "bookmark")]
        kind: String,

        /// Investigation session name
        #[arg(long)]
        session: Option<String>,

        /// Parent trace entry id
        #[arg(long)]
        parent: Option<String>,

        /// Branch name within a trace session
        #[arg(long)]
        branch: Option<String>,

        /// Tag to attach to the trace entry, repeatable
        #[arg(long = "tag")]
        tags: Vec<String>,
    },

    /// Record LSP/index semantic graph edges into a trace session
    Semantic {
        /// Format: "path:line" or "path:line:column"
        target: Option<String>,

        /// Read additional semantic trace targets from a file, one path:line[:column] per line
        #[arg(long)]
        targets_file: Option<String>,

        /// Build semantic trace targets from a query expression
        #[arg(long)]
        from_query: Option<String>,

        /// Query source for --from-query: all, index, trace, semantic, or auto
        #[arg(long, default_value = "index")]
        query_source: String,

        /// Maximum query matches to convert into semantic trace targets
        #[arg(long, default_value_t = 20)]
        query_limit: usize,

        /// Relation: references, definition, type, implementation, incoming, outgoing
        #[arg(short, long, default_value = "outgoing")]
        relation: String,

        /// Investigation session name
        #[arg(long)]
        session: Option<String>,

        /// Parent trace entry id for the semantic root
        #[arg(long)]
        parent: Option<String>,

        /// Branch name within a trace session
        #[arg(long)]
        branch: Option<String>,

        /// Tag to attach to every generated trace entry, repeatable
        #[arg(long = "tag")]
        tags: Vec<String>,

        /// Maximum relation depth to keep; semantic queries currently return one LSP hop
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Maximum outgoing edges per source; 0 means unlimited
        #[arg(long, default_value_t = 0)]
        fanout: usize,

        /// Exclude edges whose source, target, kind, or detail contains this text
        #[arg(long = "exclude")]
        exclude: Vec<String>,

        /// Fallback provider when LSP fails or returns no edges: none or index
        #[arg(long, default_value = "index")]
        fallback: String,

        /// Read/write a workspace semantic graph cache for repeated targets
        #[arg(long)]
        cache: bool,

        /// Ignore any existing semantic graph cache entry and rewrite it
        #[arg(long)]
        refresh_cache: bool,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// List trace history
    List {
        /// Filter by session
        #[arg(long)]
        session: Option<String>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Filter by kind
        #[arg(long)]
        kind: Option<String>,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Filter by priority
        #[arg(long)]
        priority: Option<String>,
    },

    /// Set or clear a trace entry note; use id from list or "latest"
    Note {
        /// Trace entry id, or "latest"
        id: String,

        /// Note text; use "-" to clear
        note: String,
    },

    /// Set or clear a trace entry status; use id from list or "latest"
    Status {
        /// Trace entry id, or "latest"
        id: String,

        /// Status value; use "-" to clear
        status: String,
    },

    /// Set or clear a trace entry priority; use id from list or "latest"
    Priority {
        /// Trace entry id, or "latest"
        id: String,

        /// Priority value; use "-" to clear
        priority: String,
    },

    /// List trace investigation sessions
    Sessions {
        /// Include archived sessions
        #[arg(long)]
        archived: bool,
    },

    /// Set the active trace investigation session for later trace commands
    Use {
        /// Trace session name
        session: String,
    },

    /// Show the active trace investigation session
    Current,

    /// Archive a trace investigation session
    Archive {
        /// Trace session name
        session: String,
    },

    /// Restore an archived trace investigation session
    Unarchive {
        /// Trace session name
        session: String,
    },

    /// Export one trace session as markdown or json
    Report {
        /// Trace session name
        session: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Export one trace session timeline as markdown or json
    Timeline {
        /// Trace session name
        session: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Replay one trace session as ordered investigation steps
    Replay {
        /// Trace session name
        session: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Export replay commands that can reconstruct a trace investigation path
    ReplayPlan {
        /// Trace session name
        session: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Optional program binary for generated DAP profile commands
        #[arg(long)]
        program: Option<String>,

        /// Optional generated DAP profile name
        #[arg(long)]
        name: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Export the structured hypotheses/evidence/conclusions/open questions for one trace session
    Structured {
        /// Trace session name
        session: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Export an investigation insights report for one trace session
    Insights {
        /// Trace session name
        session: String,

        /// Target workspace directory; enables index-backed symbol correlation
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Diff two trace sessions as markdown or json
    Diff {
        /// Left trace session name
        left: String,

        /// Right trace session name
        right: String,

        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Entry class to diff: all, semantic, bookmark, or debug
        #[arg(long, default_value = "all")]
        filter: String,
    },

    /// Rename a trace investigation session
    Rename {
        /// Existing session name
        from: String,

        /// New session name
        to: String,
    },

    /// Merge all entries from one session into another session
    Merge {
        /// Source session name
        from: String,

        /// Destination session name
        to: String,
    },

    /// Split tagged entries from one session into another session
    Split {
        /// Source session name
        from: String,

        /// Destination session name
        to: String,

        /// Tag used to select entries to move
        #[arg(long)]
        tag: String,
    },

    /// Verify trace store ids, parents, archives, and referenced paths
    Verify {
        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Exit with an error if problems are found
        #[arg(long)]
        strict: bool,
    },

    /// Repair trace store ids, dangling parents, archives, and workspace paths
    Repair {
        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Compact exact duplicate trace entries
    Compact {
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Open trace history in the picker
    Open,

    /// Clear trace history
    Clear,

    /// Export trace history as markdown
    Export {
        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },

    /// Export trace history as a parent/child graph
    Graph {
        /// Target workspace directory; omit for global trace
        #[arg(short, long)]
        directory: Option<String>,

        /// Export format: text, json, mermaid, or dot
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Filter by session
        #[arg(long)]
        session: Option<String>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Filter by kind
        #[arg(long)]
        kind: Option<String>,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Filter by priority
        #[arg(long)]
        priority: Option<String>,

        /// Filter by semantic relation
        #[arg(long)]
        relation: Option<String>,

        /// Collapse groups larger than this size by session/kind/path; 0 disables collapse
        #[arg(long, default_value_t = 0)]
        collapse_threshold: usize,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProjectAction {
    /// List configured global and project actions
    List {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// Run a configured action with template variables expanded
    Run {
        /// Action name
        name: String,

        /// Target workspace directory
        #[arg(long)]
        directory: Option<String>,

        /// Value for {file}
        #[arg(long)]
        file: Option<String>,

        /// Value for {line}
        #[arg(long)]
        line: Option<usize>,

        /// Value for {symbol}
        #[arg(long)]
        symbol: Option<String>,

        /// Print the expanded command instead of executing it
        #[arg(long)]
        dry_run: bool,

        /// Extra arguments appended after configured args
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// List built-in action templates
    Templates,

    /// Initialize project actions from a built-in template
    Init {
        /// Built-in template name
        template: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Overwrite an existing .fcs.toml
        #[arg(long)]
        force: bool,

        /// Print the generated .fcs.toml instead of writing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Validate configured global and project actions
    Doctor {
        /// Target workspace directory
        directory: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginAction {
    /// List discovered plugins
    List {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// Show one plugin manifest summary
    Show {
        /// Plugin name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Validate discovered plugin manifests
    Doctor {
        /// Target workspace directory
        directory: Option<String>,

        /// Exit with an error when warnings are found
        #[arg(long)]
        strict: bool,
    },

    /// Print the supported plugin manifest schema example
    Schema {
        /// Output format: toml, text, or json
        #[arg(short, long, default_value = "toml")]
        format: String,
    },

    /// List plugin project action templates
    Templates {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// List plugin commands
    Commands {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// Initialize .fcs.toml from a plugin template
    Init {
        /// Template selector, either name or plugin:name
        template: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Overwrite an existing .fcs.toml
        #[arg(long)]
        force: bool,

        /// Print generated .fcs.toml instead of writing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Run or print a plugin command
    Run {
        /// Command selector, either name or plugin:name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Value for {file}
        #[arg(long)]
        file: Option<String>,

        /// Value for {line}
        #[arg(long)]
        line: Option<usize>,

        /// Value for {symbol}
        #[arg(long)]
        symbol: Option<String>,

        /// Print the expanded command instead of executing it
        #[arg(long)]
        dry_run: bool,

        /// Custom template variable assignment used as {var.KEY}
        #[arg(long = "var")]
        vars: Vec<String>,

        /// Extra arguments appended after configured args
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Print the full plugin command execution plan without running it
    Plan {
        /// Command selector, either name or plugin:name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Value for {file}
        #[arg(long)]
        file: Option<String>,

        /// Value for {line}
        #[arg(long)]
        line: Option<usize>,

        /// Value for {symbol}
        #[arg(long)]
        symbol: Option<String>,

        /// Custom template variable assignment used as {var.KEY}
        #[arg(long = "var")]
        vars: Vec<String>,

        /// Extra arguments appended after configured args
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum LspAction {
    /// Show provider availability for a workspace or file
    Health {
        /// Target workspace directory
        directory: Option<String>,

        /// Optional source file to select a provider by file type
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Show document highlights for path:line[:column]
    Highlights {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Show grouped references for path:line[:column]
    Refs {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Preview LSP rename edits without applying them
    Rename {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Replacement symbol name
        new_name: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Apply the rename edits instead of printing a preview
        #[arg(long)]
        apply: bool,

        /// With --apply, show the apply report without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// List or apply code actions for path:line[:column]
    CodeActions {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Apply a 1-based code action index
        #[arg(long)]
        apply: Option<usize>,

        /// With --apply, show the apply report without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// Run source.organizeImports code action for a file
    OrganizeImports {
        /// Source file
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Apply the first organize-imports edit
        #[arg(long)]
        apply: bool,

        /// With --apply, show the apply report without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// Show a nested document outline
    Outline {
        /// Source file
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: tree or json
        #[arg(short, long, default_value = "tree")]
        format: String,
    },

    /// Show symbol breadcrumbs for path:line[:column]
    Breadcrumbs {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Show semantic tokens for a file
    SemanticTokens {
        /// Source file
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,

        /// Optional 1-based line filter
        #[arg(long)]
        line: Option<usize>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Show incoming and outgoing calls grouped around path:line[:column]
    CallTree {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DapAction {
    /// List known DAP adapter commands and availability
    Adapters {
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// List adapter-specific DAP launch/attach templates
    Templates {
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Print a DAP launch request for an executable
    Launch {
        /// Program binary to launch
        program: String,

        /// DAP adapter type label, e.g. cppdbg or codelldb
        #[arg(short, long, default_value = "cppdbg")]
        adapter: String,

        /// DAP request type: launch or attach
        #[arg(long, default_value = "launch")]
        request: String,

        /// Process id for attach requests
        #[arg(long = "process-id")]
        process_id: Option<u64>,

        /// Launch profile name; defaults to the program file name
        #[arg(short, long)]
        name: Option<String>,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Breakpoint condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-condition")]
        break_conditions: Vec<String>,

        /// Breakpoint hit condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-hit")]
        break_hits: Vec<String>,

        /// Breakpoint log message, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-log")]
        break_logs: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Stop immediately after launch
        #[arg(long)]
        stop_on_entry: bool,

        /// Print setBreakpoints requests before the launch request
        #[arg(long)]
        bundle: bool,

        /// Program arguments after launch setup
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Save a named DAP launch profile in the workspace cache
    SaveProfile {
        /// Profile name
        name: String,

        /// Program binary to launch
        program: String,

        /// DAP adapter type label, e.g. cppdbg or codelldb
        #[arg(short, long, default_value = "cppdbg")]
        adapter: String,

        /// DAP request type: launch or attach
        #[arg(long, default_value = "launch")]
        request: String,

        /// Process id for attach requests
        #[arg(long = "process-id")]
        process_id: Option<u64>,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Breakpoint condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-condition")]
        break_conditions: Vec<String>,

        /// Breakpoint hit condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-hit")]
        break_hits: Vec<String>,

        /// Breakpoint log message, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-log")]
        break_logs: Vec<String>,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Stop immediately after launch
        #[arg(long)]
        stop_on_entry: bool,

        /// Program arguments after launch setup
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// List saved DAP launch profiles
    Profiles {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// Diagnose saved DAP profiles, adapter availability, and launch/attach inputs
    Doctor {
        /// Target workspace directory
        directory: Option<String>,

        /// Restrict diagnostics to one profile name
        #[arg(short, long)]
        name: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Save a DAP launch profile from all line locations in a trace session
    FromTrace {
        /// Trace session name
        session: String,

        /// Program binary to launch
        program: String,

        /// Saved DAP profile name; defaults to <session>-dap
        #[arg(short, long)]
        name: Option<String>,

        /// DAP adapter type label, e.g. cppdbg or codelldb
        #[arg(short, long, default_value = "cppdbg")]
        adapter: String,

        /// DAP request type: launch or attach
        #[arg(long, default_value = "launch")]
        request: String,

        /// Process id for attach requests
        #[arg(long = "process-id")]
        process_id: Option<u64>,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Stop immediately after launch
        #[arg(long)]
        stop_on_entry: bool,

        /// Program arguments after launch setup
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Print a saved DAP launch request
    RequestProfile {
        /// Profile name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Print setBreakpoints requests before the launch request
        #[arg(long)]
        bundle: bool,
    },

    /// Export a repeatable mock DAP request/event transcript for a saved profile
    Transcript {
        /// Profile name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Run a non-interactive DAP client session against the built-in mock adapter
    SessionSmoke {
        /// Program binary to launch in the mock session
        program: String,

        /// DAP adapter type label used in initialize arguments
        #[arg(short, long, default_value = "mock")]
        adapter: String,

        /// DAP request type: launch or attach
        #[arg(long, default_value = "launch")]
        request: String,

        /// Process id for attach requests
        #[arg(long = "process-id")]
        process_id: Option<u64>,

        /// Launch profile name; defaults to the program file name
        #[arg(short, long)]
        name: Option<String>,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Breakpoint condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-condition")]
        break_conditions: Vec<String>,

        /// Breakpoint hit condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-hit")]
        break_hits: Vec<String>,

        /// Breakpoint log message, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-log")]
        break_logs: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Stop immediately after launch
        #[arg(long)]
        stop_on_entry: bool,

        /// Program arguments after launch setup
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Run a real DAP initialize/launch/configurationDone session against an adapter process
    AdapterSession {
        /// DAP adapter executable command
        adapter_command: String,

        /// Program binary to launch
        program: String,

        /// DAP adapter type label used in initialize arguments
        #[arg(short, long, default_value = "cppdbg")]
        adapter: String,

        /// DAP request type: launch or attach
        #[arg(long, default_value = "launch")]
        request: String,

        /// Process id for attach requests
        #[arg(long = "process-id")]
        process_id: Option<u64>,

        /// Launch profile name; defaults to the program file name
        #[arg(short, long)]
        name: Option<String>,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Breakpoint condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-condition")]
        break_conditions: Vec<String>,

        /// Breakpoint hit condition, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-hit")]
        break_hits: Vec<String>,

        /// Breakpoint log message, repeatable; one value applies to all breakpoints, otherwise by index
        #[arg(long = "break-log")]
        break_logs: Vec<String>,

        /// Working directory for the debugged program and adapter process
        #[arg(long)]
        cwd: Option<String>,

        /// Adapter environment assignment, repeatable: KEY=VALUE
        #[arg(long = "adapter-env")]
        adapter_env: Vec<String>,

        /// Debuggee environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Stop immediately after launch
        #[arg(long)]
        stop_on_entry: bool,

        /// Program arguments after launch setup
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum HistoryAction {
    /// List query history
    List,

    /// Clear query history
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum DebugAction {
    /// Build a debugger command with explicit breakpoints
    Command {
        /// Program binary to debug
        binary: String,

        /// Debugger backend: gdb or lldb
        #[arg(short, long, default_value = "gdb")]
        debugger: String,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Program arguments after debugger setup
        #[arg(last = true)]
        args: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Launch the debugger instead of printing the command
        #[arg(long)]
        run: bool,
    },

    /// Build a debugger command using the latest trace entry as breakpoint
    Last {
        /// Program binary to debug
        binary: String,

        /// Debugger backend: gdb or lldb
        #[arg(short, long, default_value = "gdb")]
        debugger: String,

        /// Program arguments after debugger setup
        #[arg(last = true)]
        args: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Launch the debugger instead of printing the command
        #[arg(long)]
        run: bool,
    },

    /// Save a named debugger profile in the workspace cache
    SaveProfile {
        /// Profile name
        name: String,

        /// Program binary to debug
        binary: String,

        /// Debugger backend: gdb or lldb
        #[arg(short, long, default_value = "gdb")]
        debugger: String,

        /// Breakpoint location, repeatable: path:line[:column]
        #[arg(short = 'b', long = "break")]
        breakpoints: Vec<String>,

        /// Target workspace directory
        #[arg(long)]
        directory: Option<String>,

        /// Program arguments after debugger setup
        #[arg(last = true)]
        args: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,
    },

    /// List saved debugger profiles
    Profiles {
        /// Target workspace directory
        directory: Option<String>,
    },

    /// Save a debugger profile from all line locations in a trace session
    FromTrace {
        /// Trace session name
        session: String,

        /// Program binary to debug
        binary: String,

        /// Saved profile name; defaults to <session>-debug
        #[arg(short, long)]
        name: Option<String>,

        /// Debugger backend: gdb or lldb
        #[arg(short, long, default_value = "gdb")]
        debugger: String,

        /// Target workspace directory
        #[arg(long)]
        directory: Option<String>,

        /// Program arguments after debugger setup
        #[arg(last = true)]
        args: Vec<String>,

        /// Working directory for the debugged program
        #[arg(long)]
        cwd: Option<String>,

        /// Environment assignment, repeatable: KEY=VALUE
        #[arg(long = "env")]
        env: Vec<String>,

        /// Launch the debugger after saving the profile
        #[arg(long)]
        run: bool,
    },

    /// Delete a saved debugger profile
    DeleteProfile {
        /// Profile name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Enable a saved profile breakpoint by 1-based index
    EnableBreakpoint {
        /// Profile name
        name: String,

        /// 1-based breakpoint index
        index: usize,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Disable a saved profile breakpoint by 1-based index
    DisableBreakpoint {
        /// Profile name
        name: String,

        /// 1-based breakpoint index
        index: usize,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Run or print a saved debugger profile
    RunProfile {
        /// Profile name
        name: String,

        /// Target workspace directory
        #[arg(short, long)]
        directory: Option<String>,

        /// Launch the debugger instead of printing the command
        #[arg(long)]
        run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorkspaceAction {
    /// Show workspace readiness for semantic navigation
    Status {
        /// Target directory
        directory: Option<String>,
    },

    /// Create non-intrusive fcs workspace cache metadata
    Init {
        /// Target directory
        directory: Option<String>,
    },

    /// Write a project-level .fcs.toml template
    Config {
        /// Target directory
        directory: Option<String>,

        /// Overwrite existing .fcs.toml
        #[arg(long)]
        force: bool,
    },

    /// Manage named workspace profiles for monorepos and repeated roots
    Profile {
        #[command(subcommand)]
        action: WorkspaceProfileAction,
    },

    /// Validate project-level .fcs.toml configuration
    ConfigDoctor {
        /// Target directory
        directory: Option<String>,

        /// Exit with an error when warnings are found
        #[arg(long)]
        strict: bool,
    },

    /// Print the supported project .fcs.toml schema example
    ConfigSchema {
        /// Output format: toml, text, or json
        #[arg(short, long, default_value = "toml")]
        format: String,
    },

    /// Print project detection and actionable setup advice
    Advise {
        /// Target directory
        directory: Option<String>,
    },

    /// Print the non-blocking startup plan used by the TUI activity panel
    Plan {
        /// Target directory
        directory: Option<String>,
    },

    /// Print detected project profile without advice text
    Detect {
        /// Target directory
        directory: Option<String>,
    },

    /// Run workspace readiness, cache, config, and release health checks
    Doctor {
        /// Target directory
        directory: Option<String>,
    },

    /// Build a support bundle with workspace, index, LSP, DAP, workflow, and query diagnostics
    DoctorBundle {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Write the bundle to this path instead of stdout
        #[arg(long)]
        out: Option<String>,
    },

    /// Print diagnostic workflow templates for this workspace
    Workflows {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorkspaceProfileAction {
    /// Save a named workspace profile
    Save {
        /// Profile name
        name: String,

        /// Target directory
        directory: Option<String>,

        /// Optional profile description
        #[arg(short, long)]
        description: Option<String>,

        /// Profile index root, repeatable; defaults to detected roots
        #[arg(long = "index-root")]
        index_roots: Vec<String>,
    },

    /// List saved workspace profiles
    List,

    /// Show one saved workspace profile
    Show {
        /// Profile name
        name: String,
    },

    /// Mark a saved workspace profile as active
    Use {
        /// Profile name
        name: String,
    },

    /// Show the active workspace profile
    Current,

    /// Delete a saved workspace profile
    Delete {
        /// Profile name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    /// Run a foreground polling service that refreshes index and writes a unified snapshot
    Start {
        /// Target directory
        directory: Option<String>,

        /// Milliseconds between refresh checks
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,

        /// Stop after this many cycles; omit to run until interrupted
        #[arg(long)]
        max_cycles: Option<usize>,

        /// Print each completed cycle
        #[arg(long)]
        foreground: bool,

        /// File traversal options used by index refresh
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Show the latest service heartbeat
    Status {
        /// Target directory
        directory: Option<String>,
    },

    /// Build and print a unified status snapshot once
    Snapshot {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Query through the same advanced query engine used by service snapshots
    Query {
        /// Query expression
        expression: String,

        /// Target directory
        directory: Option<String>,

        /// Query source: index, trace, or all
        #[arg(short, long, default_value = "all")]
        source: String,

        /// Query matching mode: fuzzy, exact, or regex
        #[arg(long, default_value = "fuzzy")]
        mode: String,

        /// Apply a built-in query macro, e.g. functions, tests, todo
        #[arg(long = "macro")]
        macros: Vec<String>,

        /// Maximum entries to print
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Print the parsed query plan instead of running it
        #[arg(long)]
        explain: bool,

        /// Print query latency in milliseconds
        #[arg(long)]
        timing: bool,

        /// Print a warning when query latency exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Append score and mode details to each match
        #[arg(long)]
        score_explain: bool,
    },

    /// Request a running foreground service to stop
    Stop {
        /// Target directory
        directory: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum BenchAction {
    /// Run the standard benchmark suite for a workspace
    All {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when a probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Maximum entries for index/query probes
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Query text used by search and index probes
        #[arg(long, default_value = "main")]
        query: String,

        /// File traversal options used by search/index probes
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Measure ripgrep-style search latency
    Search {
        /// Search pattern
        pattern: String,

        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when the probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Ripgrep-compatible search options
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Measure cached index latency
    Index {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when a probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Include an index rebuild in the measurement
        #[arg(long)]
        build: bool,

        /// Maximum entries for list/query probes
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Query text for the query latency probe
        #[arg(long, default_value = "main")]
        query: String,

        /// File traversal options used only with --build
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Measure trace store latency
    Trace {
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when a probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,
    },

    /// Measure non-interactive TUI source loading latency
    Tui {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when a probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,

        /// Query text used for symbol source filtering
        #[arg(long, default_value = "main")]
        query: String,

        /// File traversal options used by file/symbol source probes
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Measure preview target file-read latency
    Preview {
        /// Format: path:line
        target: String,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn when the probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,
    },

    /// Save the latest workspace benchmark report as the baseline
    Baseline {
        /// Target directory
        directory: Option<String>,
    },

    /// Compare the latest workspace benchmark report against the saved baseline
    Compare {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Allowed absolute slowdown in milliseconds
        #[arg(long, default_value_t = 10)]
        threshold_ms: u128,

        /// Allowed relative slowdown percentage
        #[arg(long, default_value_t = 25)]
        threshold_percent: u128,

        /// Exit with an error if regressions are found
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum IndexAction {
    /// Show cached index freshness and counts
    Status {
        /// Target directory
        directory: Option<String>,
    },

    /// Show cached index size and distribution statistics
    Stats {
        /// Target directory
        directory: Option<String>,
    },

    /// Show index shard planning guidance for large workspaces
    Shards {
        /// Target directory
        directory: Option<String>,

        /// Target symbol count per recommended shard
        #[arg(long, default_value_t = 5000)]
        target_symbols: usize,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Write shard cache files and a manifest
        #[arg(long)]
        write: bool,
    },

    /// Show shard cache freshness
    ShardStatus {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Query cached index shards, falling back to the main index when stale
    ShardQuery {
        /// Query text
        query: String,

        /// Target directory
        directory: Option<String>,

        /// Entry kind: files or symbols
        #[arg(short, long, default_value = "symbols")]
        kind: String,

        /// Maximum entries to print
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Print query latency
        #[arg(long)]
        timing: bool,

        /// Warn on stderr when query latency exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u64>,
    },

    /// Rebuild the cached files/symbols index
    Build {
        /// Target directory
        directory: Option<String>,

        /// File traversal options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// List cached index entries
    List {
        /// Target directory
        directory: Option<String>,

        /// Entry kind: files or symbols
        #[arg(short, long, default_value = "symbols")]
        kind: String,

        /// Maximum entries to print
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// Rewrite cached index TOML in compact form
    Compact {
        /// Target directory
        directory: Option<String>,

        /// Report size changes without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Load the index once to warm filesystem cache
    Prewarm {
        /// Target directory
        directory: Option<String>,
    },

    /// Rebuild only when the index is missing, stale, corrupt, or schema-migrated
    Refresh {
        /// Target directory
        directory: Option<String>,

        /// File traversal options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Run a polling daemon that keeps the cached index fresh
    Daemon {
        /// Target directory
        directory: Option<String>,

        /// Milliseconds between refresh checks
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,

        /// Stop after this many cycles; omit to run until interrupted
        #[arg(long)]
        max_cycles: Option<usize>,

        /// Run in the foreground and print each completed cycle
        #[arg(long)]
        foreground: bool,

        /// File traversal options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },

    /// Show the last index daemon heartbeat
    DaemonStatus {
        /// Target directory
        directory: Option<String>,
    },

    /// Query cached index entries with fuzzy substring scoring
    Query {
        /// Query text
        query: String,

        /// Target directory
        directory: Option<String>,

        /// Entry kind: files or symbols
        #[arg(short, long, default_value = "symbols")]
        kind: String,

        /// Maximum entries to print
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Print query latency
        #[arg(long)]
        timing: bool,

        /// Warn on stderr when query latency exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u64>,
    },

    /// Profile cached index load/list/query/shard latency
    Profile {
        /// Query text for the query probe
        query: String,

        /// Target directory
        directory: Option<String>,

        /// Entry kind: files or symbols
        #[arg(short, long, default_value = "symbols")]
        kind: String,

        /// Maximum entries for list/query probes
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Warn on stderr when any probe exceeds this threshold
        #[arg(long)]
        warn_ms: Option<u128>,
    },

    /// Diagnose index freshness, schema, and corruption status
    Doctor {
        /// Target directory
        directory: Option<String>,
    },

    /// Rebuild stale or corrupt index data
    Repair {
        /// Target directory
        directory: Option<String>,

        /// File traversal options (e.g. --hidden, --no-ignore, -L, -d 2)
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,

        /// Rebuild even when the index is already healthy
        #[arg(long)]
        force: bool,
    },

    /// Verify main index and shard cache health without rebuilding
    Verify {
        /// Target directory
        directory: Option<String>,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Measure cached index operation latency
    Bench {
        /// Target directory
        directory: Option<String>,

        /// Include an index rebuild in the measurement
        #[arg(long)]
        build: bool,

        /// Maximum entries for list/query probes
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Query text for the query latency probe
        #[arg(long, default_value = "main")]
        query: String,

        /// File traversal options used only with --build
        #[arg(short, long, allow_hyphen_values = true)]
        option: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GraphAction {
    /// Build an LSP-backed graph from a source location
    Semantic {
        /// Format: "path:line" or "path:line:column"
        target: String,

        /// Relation: references, definition, type, implementation, incoming, outgoing
        #[arg(short, long, default_value = "outgoing")]
        relation: String,

        /// Output format: text, json, mermaid, or dot
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Maximum relation depth to keep; semantic queries currently return one LSP hop
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Maximum outgoing edges per source; 0 means unlimited
        #[arg(long, default_value_t = 0)]
        fanout: usize,

        /// Exclude edges whose source, target, kind, or detail contains this text
        #[arg(long = "exclude")]
        exclude: Vec<String>,

        /// Fallback provider when LSP fails or returns no edges: none or index
        #[arg(long, default_value = "none")]
        fallback: String,

        /// Read/write a workspace semantic graph cache for repeated targets
        #[arg(long)]
        cache: bool,

        /// Ignore any existing semantic graph cache entry and rewrite it
        #[arg(long)]
        refresh_cache: bool,

        /// Workspace directory override
        #[arg(short, long)]
        directory: Option<String>,
    },

    /// Build a lightweight include/import graph from project files
    Imports {
        /// Target directory
        directory: Option<String>,

        /// Maximum files to scan
        #[arg(short, long, default_value_t = 500)]
        limit: usize,

        /// Output format: text, json, mermaid, or dot
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Maximum local import expansion depth
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Maximum outgoing imports per source file; 0 means unlimited
        #[arg(long, default_value_t = 0)]
        fanout: usize,

        /// Exclude edges whose source, target, kind, or detail contains this text
        #[arg(long = "exclude")]
        exclude: Vec<String>,
    },

    /// Build a lightweight Rust module graph from project files
    Modules {
        /// Target directory
        directory: Option<String>,

        /// Maximum files to scan
        #[arg(short, long, default_value_t = 500)]
        limit: usize,

        /// Output format: text, json, mermaid, or dot
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Maximum local module expansion depth
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Maximum outgoing modules per source file; 0 means unlimited
        #[arg(long, default_value_t = 0)]
        fanout: usize,

        /// Exclude edges whose source, target, kind, or detail contains this text
        #[arg(long = "exclude")]
        exclude: Vec<String>,
    },

    /// Build a lightweight offline call graph from project files
    Calls {
        /// Target directory
        directory: Option<String>,

        /// Maximum files to scan
        #[arg(short, long, default_value_t = 500)]
        limit: usize,

        /// Output format: text, json, mermaid, or dot
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Maximum relation depth to keep
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Maximum outgoing calls per source location; 0 means unlimited
        #[arg(long, default_value_t = 0)]
        fanout: usize,

        /// Exclude edges whose source, target, kind, or detail contains this text
        #[arg(long = "exclude")]
        exclude: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum IgnoreAction {
    /// Initialize a .ignore file with default patterns
    Init,

    /// Add patterns to .ignore
    Add {
        /// Patterns to add
        patterns: Vec<String>,
    },

    /// Remove patterns from .ignore
    Remove {
        /// Patterns to remove
        patterns: Vec<String>,
    },

    /// List patterns in .ignore
    List,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn clap_command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn release_smoke_commands_parse() {
        let cases: &[&[&str]] = &[
            &["fcs", "tui", "--mode", "files", "--query", "main"],
            &[
                "fcs",
                "tui-script",
                "script.fcs",
                ".",
                "--mode",
                "symbols",
                "--query",
                "main",
                "--format",
                "json",
                "--step-timeout-ms",
                "100",
            ],
            &["fcs", "workspace", "status", "."],
            &["fcs", "workspace", "detect", "."],
            &["fcs", "workspace", "doctor", "."],
            &["fcs", "workspace", "doctor-bundle", ".", "--format", "json"],
            &["fcs", "workspace", "config-doctor", ".", "--strict"],
            &["fcs", "workspace", "config-schema", "--format", "json"],
            &[
                "fcs",
                "workspace",
                "profile",
                "save",
                "main",
                ".",
                "--description",
                "main workspace",
                "--index-root",
                "src",
            ],
            &["fcs", "workspace", "profile", "list"],
            &["fcs", "workspace", "profile", "show", "main"],
            &["fcs", "workspace", "profile", "use", "main"],
            &["fcs", "workspace", "profile", "current"],
            &["fcs", "workspace", "profile", "delete", "main"],
            &["fcs", "service", "snapshot", ".", "--format", "json"],
            &["fcs", "service", "status", "."],
            &[
                "fcs",
                "service",
                "start",
                ".",
                "--interval-ms",
                "0",
                "--max-cycles",
                "1",
                "--foreground",
            ],
            &[
                "fcs",
                "service",
                "query",
                "kind:function text:main",
                ".",
                "--source",
                "index",
                "--mode",
                "exact",
                "--score-explain",
            ],
            &["fcs", "service", "stop", "."],
            &["fcs", "index", "doctor", "."],
            &["fcs", "index", "verify", ".", "--format", "json"],
            &["fcs", "index", "stats", "."],
            &[
                "fcs",
                "index",
                "shards",
                ".",
                "--target-symbols",
                "1000",
                "--format",
                "json",
            ],
            &[
                "fcs",
                "index",
                "shards",
                ".",
                "--target-symbols",
                "1000",
                "--format",
                "json",
                "--write",
            ],
            &["fcs", "index", "shard-status", ".", "--format", "json"],
            &[
                "fcs",
                "index",
                "shard-query",
                "main",
                ".",
                "--kind",
                "symbols",
                "--limit",
                "5",
                "--timing",
            ],
            &["fcs", "index", "compact", ".", "--dry-run"],
            &["fcs", "index", "prewarm", "."],
            &["fcs", "index", "refresh", "."],
            &[
                "fcs",
                "index",
                "daemon",
                ".",
                "--interval-ms",
                "0",
                "--max-cycles",
                "1",
                "--foreground",
            ],
            &["fcs", "index", "daemon-status", "."],
            &["fcs", "index", "query", "main", ".", "--timing", "--warn-ms", "1000"],
            &[
                "fcs",
                "index",
                "profile",
                "main",
                ".",
                "--kind",
                "symbols",
                "--format",
                "json",
                "--warn-ms",
                "1000",
            ],
            &["fcs", "index", "bench", ".", "--query", "main"],
            &[
                "fcs",
                "query",
                "kind:function lang:rust text:main",
                ".",
                "--source",
                "all",
                "--format",
                "json",
            ],
            &[
                "fcs",
                "query",
                "name:smoke_.*",
                ".",
                "--source",
                "index",
                "--mode",
                "regex",
                "--macro",
                "functions",
                "--score-explain",
            ],
            &[
                "fcs",
                "query",
                "kind:function text:main",
                ".",
                "--source",
                "index",
                "--profile",
                "--format",
                "json",
            ],
            &[
                "fcs",
                "query",
                "kind:function name:main",
                ".",
                "--source",
                "index",
                "--mode",
                "exact",
                "--save",
                "main",
            ],
            &["fcs", "query", "--use", "main", "--source", "index"],
            &["fcs", "query", "--list-saved"],
            &["fcs", "query", "--delete-saved", "main"],
            &[
                "fcs",
                "query",
                "kind:function (name:main or name:init) not path:target",
                ".",
                "--source",
                "semantic",
                "--explain",
            ],
            &["fcs", "bench", "all", ".", "--format", "json", "--warn-ms", "10000"],
            &["fcs", "bench", "search", "main", ".", "--format", "json"],
            &["fcs", "bench", "index", ".", "--query", "main", "--format", "json"],
            &["fcs", "bench", "tui", ".", "--query", "main", "--format", "json"],
            &["fcs", "bench", "trace", "--format", "json"],
            &["fcs", "bench", "preview", "src/main.rs:1", "--format", "json"],
            &["fcs", "trace", "export", "--format", "json"],
            &["fcs", "trace", "graph", "--directory", ".", "--format", "mermaid"],
            &["fcs", "trace", "use", "smoke"],
            &["fcs", "trace", "current"],
            &[
                "fcs",
                "trace",
                "semantic",
                "src/main.rs:1:1",
                "--relation",
                "outgoing",
                "--session",
                "smoke-semantic",
                "--tag",
                "smoke",
                "--fallback",
                "index",
                "--cache",
                "--format",
                "json",
            ],
            &[
                "fcs",
                "trace",
                "semantic",
                "--from-query",
                "kind:function name:main",
                "--query-source",
                "index",
                "--query-limit",
                "3",
                "--directory",
                ".",
            ],
            &[
                "fcs",
                "trace",
                "semantic",
                "--targets-file",
                "targets.txt",
                "--relation",
                "references",
                "--session",
                "batch",
            ],
            &["fcs", "trace", "note", "latest", "checked"],
            &["fcs", "trace", "status", "latest", "open"],
            &["fcs", "trace", "priority", "latest", "high"],
            &["fcs", "trace", "timeline", "smoke", "--format", "json"],
            &["fcs", "trace", "replay", "smoke", "--format", "json"],
            &[
                "fcs",
                "trace",
                "replay-plan",
                "smoke",
                "--format",
                "json",
                "--program",
                "target/debug/fcs",
                "--name",
                "smoke-replay",
            ],
            &["fcs", "trace", "diff", "smoke-a", "smoke-b", "--format", "json"],
            &[
                "fcs",
                "trace",
                "insights",
                "smoke",
                "--directory",
                ".",
                "--format",
                "json",
            ],
            &["fcs", "actions", "list", "."],
            &[
                "fcs",
                "actions",
                "run",
                "test",
                "--directory",
                ".",
                "--file",
                "src/main.rs",
                "--line",
                "1",
                "--symbol",
                "main",
                "--dry-run",
                "--",
                "--exact",
            ],
            &["fcs", "plugin", "list", "."],
            &["fcs", "plugin", "show", "builtin-dev", "--directory", "."],
            &["fcs", "plugin", "doctor", "."],
            &["fcs", "plugin", "doctor", ".", "--strict"],
            &["fcs", "plugin", "schema", "--format", "toml"],
            &["fcs", "plugin", "templates", "."],
            &["fcs", "plugin", "commands", "."],
            &[
                "fcs",
                "plugin",
                "init",
                "builtin-dev:rust-debug",
                "--directory",
                ".",
                "--dry-run",
            ],
            &[
                "fcs",
                "plugin",
                "run",
                "builtin-dev:cargo-check",
                "--directory",
                ".",
                "--dry-run",
                "--var",
                "mode=debug",
                "--",
                "--locked",
            ],
            &[
                "fcs",
                "plugin",
                "plan",
                "builtin-dev:cargo-check",
                "--directory",
                ".",
                "--var",
                "mode=debug",
                "--",
                "--locked",
            ],
            &[
                "fcs",
                "graph",
                "imports",
                ".",
                "--format",
                "mermaid",
                "--depth",
                "2",
                "--fanout",
                "4",
                "--exclude",
                "target",
            ],
            &[
                "fcs", "graph", "modules", ".", "--format", "dot", "--depth", "2", "--fanout", "4",
            ],
            &[
                "fcs", "graph", "calls", ".", "--format", "json", "--depth", "1", "--fanout", "8",
            ],
            &[
                "fcs",
                "graph",
                "semantic",
                "src/main.rs:1:1",
                "--relation",
                "references",
                "--format",
                "dot",
                "--depth",
                "1",
                "--fanout",
                "8",
                "--fallback",
                "index",
                "--cache",
                "--refresh-cache",
            ],
            &["fcs", "type-def", "src/main.rs:1:1"],
            &["fcs", "doc-symbols", "src/main.rs"],
            &["fcs", "outgoing", "src/main.rs:1:1"],
            &["fcs", "lsp", "highlights", "src/main.rs:1:1", "--directory", "."],
            &["fcs", "lsp", "refs", "src/main.rs:1:1", "--directory", "."],
            &["fcs", "lsp", "rename", "src/main.rs:1:1", "renamed", "--directory", "."],
            &[
                "fcs",
                "lsp",
                "rename",
                "src/main.rs:1:1",
                "renamed",
                "--directory",
                ".",
                "--apply",
                "--dry-run",
            ],
            &["fcs", "lsp", "code-actions", "src/main.rs:1:1", "--directory", "."],
            &[
                "fcs",
                "lsp",
                "code-actions",
                "src/main.rs:1:1",
                "--directory",
                ".",
                "--format",
                "json",
                "--apply",
                "1",
                "--dry-run",
            ],
            &["fcs", "lsp", "organize-imports", "src/main.rs", "--directory", "."],
            &[
                "fcs",
                "lsp",
                "organize-imports",
                "src/main.rs",
                "--directory",
                ".",
                "--apply",
                "--dry-run",
            ],
            &["fcs", "lsp", "outline", "src/main.rs", "--format", "json"],
            &["fcs", "lsp", "breadcrumbs", "src/main.rs:1:1", "--directory", "."],
            &[
                "fcs",
                "lsp",
                "semantic-tokens",
                "src/main.rs",
                "--line",
                "1",
                "--format",
                "json",
            ],
            &["fcs", "lsp", "call-tree", "src/main.rs:1:1", "--directory", "."],
            &[
                "fcs",
                "debug",
                "command",
                "target/debug/fcs",
                "-b",
                "src/main.rs:1",
                "--cwd",
                ".",
                "--env",
                "FCS_SMOKE=1",
            ],
            &[
                "fcs",
                "debug",
                "save-profile",
                "smoke",
                "target/debug/fcs",
                "-b",
                "src/main.rs:1",
                "--directory",
                ".",
                "--cwd",
                ".",
                "--env",
                "FCS_SMOKE=1",
                "--",
                "--help",
            ],
            &["fcs", "debug", "disable-breakpoint", "smoke", "1", "--directory", "."],
            &["fcs", "debug", "enable-breakpoint", "smoke", "1", "--directory", "."],
            &["fcs", "debug", "run-profile", "smoke", "--directory", "."],
            &[
                "fcs",
                "debug",
                "from-trace",
                "smoke",
                "target/debug/fcs",
                "--name",
                "smoke-trace",
                "--directory",
                ".",
                "--cwd",
                ".",
                "--env",
                "FCS_SMOKE=1",
                "--",
                "--help",
            ],
            &["fcs", "debug", "delete-profile", "smoke", "--directory", "."],
            &["fcs", "dap", "templates", "--format", "json"],
            &["fcs", "dap", "doctor", ".", "--name", "smoke", "--format", "json"],
            &[
                "fcs",
                "dap",
                "launch",
                "target/debug/fcs",
                "--request",
                "attach",
                "--process-id",
                "1234",
                "--bundle",
            ],
            &[
                "fcs",
                "dap",
                "session-smoke",
                "target/debug/fcs",
                "-b",
                "src/main.rs:1",
                "--cwd",
                ".",
                "--env",
                "FCS_SMOKE=1",
                "--",
                "--help",
            ],
            &[
                "fcs",
                "dap",
                "session-smoke",
                "target/debug/fcs",
                "--request",
                "attach",
                "--process-id",
                "1234",
            ],
            &[
                "fcs",
                "dap",
                "from-trace",
                "smoke",
                "target/debug/fcs",
                "--name",
                "smoke-dap",
                "--directory",
                ".",
                "--cwd",
                ".",
                "--env",
                "FCS_SMOKE=1",
                "--",
                "--help",
            ],
            &[
                "fcs",
                "dap",
                "adapter-session",
                "mock-adapter",
                "target/debug/fcs",
                "-b",
                "src/main.rs:1",
                "--cwd",
                ".",
                "--adapter-env",
                "FCS_ADAPTER=1",
                "--env",
                "FCS_SMOKE=1",
                "--",
                "--help",
            ],
            &[
                "fcs",
                "dap",
                "adapter-session",
                "mock-adapter",
                "target/debug/fcs",
                "--request",
                "attach",
                "--process-id",
                "1234",
            ],
            &["fcs", "complete", "bash"],
            &["fcs", "man", "--stdout"],
            &["fcs", "man", "--out-dir", "target/man"],
        ];

        for args in cases {
            Cli::try_parse_from(*args).unwrap_or_else(|err| panic!("failed to parse {args:?}: {err}"));
        }
    }

    #[test]
    fn release_help_lists_core_workflows() {
        let help = Cli::command().render_long_help().to_string();

        for command in [
            "tui",
            "tui-script",
            "workspace",
            "service",
            "index",
            "query",
            "bench",
            "graph",
            "trace",
            "actions",
            "plugin",
            "debug",
            "dap",
            "type-def",
            "doc-symbols",
            "outgoing",
        ] {
            assert!(help.contains(command), "help output should mention {command}");
        }
    }
}
