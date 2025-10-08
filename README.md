# Crates MCP Server

MCP server for querying Rust crates from crates.io and docs.rs. Search crates, get info, versions, dependencies, and documentation.

## Quick Start

```bash
# Build and run
cargo build --release
cargo run --release
```

## Tools

- `search_crates` - Find crates by name
- `get_crate_info` - Get details about a crate  
- `get_crate_versions` - List versions
- `get_crate_dependencies` - Show dependencies
- `get_crate_documentation` - Get docs from docs.rs

## Claude Code Integration

Add to your Claude Code MCP configuration:

**Option 1: Run directly**
```json
{
  "mcpServers": {
    "crates": {
      "command": "cargo",
      "args": ["run", "--release"],
      "cwd": "/path/to/crates-mcp"
    }
  }
}
```

**Option 2: Use binary**
```bash
# Build first
cargo build --release

# Add to config
{
  "mcpServers": {
    "crates": {
      "command": "/path/to/crates-mcp/target/release/crates-mcp"
    }
  }
}
```

**Config file locations:**
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json` 
- Linux: `~/.config/claude/claude_desktop_config.json`

Restart Claude Code after adding the config.

## Usage Examples

```
# Search for HTTP clients
> Search for "http client" crates

# Get info about reqwest
> What is the reqwest crate?

# Check tokio dependencies  
> Show me tokio's dependencies

# View serde documentation
> Show me the docs for serde
```

## Development

```bash
# Test
cargo test

# Format  
cargo fmt

# Lint
cargo clippy
```

## License

MIT Copyright (c) 2025 Pato Lankenau
