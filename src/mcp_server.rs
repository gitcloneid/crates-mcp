use anyhow::{Context, Result};
use serde_json::json;
use std::io::{self, BufRead, BufReader, Write};
use tracing::{debug, error, info};

use crate::crates_client::CratesClient;
use crate::docs_client::DocsClient;

/// MCP Server for providing Rust crate information
pub struct CratesIoMcpServer {
    crates_client: CratesClient,
    docs_client: DocsClient,
}

impl CratesIoMcpServer {
    /// Create a new MCP server instance
    pub async fn new() -> Result<Self> {
        let crates_client = CratesClient::new().await?;
        let docs_client = DocsClient::new();

        Ok(Self {
            crates_client,
            docs_client,
        })
    }

    pub async fn run(self, transport: &str) -> Result<()> {
        match transport {
            "stdio" => {
                self.run_stdio().await?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported transport: {}", transport));
            }
        }

        Ok(())
    }

    /// Helper method to create JSON-RPC error responses
    fn create_error_response(
        id: Option<serde_json::Value>,
        code: i32,
        message: &str,
    ) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message
            }
        })
    }

    /// Helper method to create JSON-RPC success responses
    fn create_success_response(
        id: Option<serde_json::Value>,
        result: serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        })
    }

    /// Run the MCP server using stdio transport
    async fn run_stdio(self) -> Result<()> {
        let stdin = io::stdin();
        let reader = BufReader::new(stdin);
        let mut stdout = io::stdout();

        // MCP server will handle initialization via JSON-RPC protocol

        info!("MCP Server starting...");

        for line in reader.lines() {
            let line = line.context("Failed to read line")?;
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    let response_str = serde_json::to_string(&response)?;
                    writeln!(stdout, "{}", response_str)?;
                    stdout.flush()?;
                }
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let error_response = Self::create_error_response(None, -32700, "Parse error");
                    let response_str = serde_json::to_string(&error_response)?;
                    writeln!(stdout, "{}", response_str)?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&self, request: serde_json::Value) -> serde_json::Value {
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = request.get("id").cloned();

        match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": env!("CARGO_PKG_NAME"),
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                Self::create_success_response(id, result)
            }
            "tools/list" => self.handle_list_tools(id).await,
            "tools/call" => self.handle_call_tool(request, id).await,
            _ => Self::create_error_response(id, -32601, "Method not found"),
        }
    }

    async fn handle_list_tools(&self, id: Option<serde_json::Value>) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "search_crates",
                        "description": "Search for Rust crates on crates.io",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search query for crates"
                                },
                                "limit": {
                                    "type": "integer",
                                    "description": "Maximum number of results to return (default: 10, max: 100)",
                                    "minimum": 1,
                                    "maximum": 100
                                }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "get_crate_info",
                        "description": "Get detailed information about a specific Rust crate",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Name of the crate to get information about"
                                }
                            },
                            "required": ["name"]
                        }
                    },
                    {
                        "name": "get_crate_versions",
                        "description": "Get version history for a Rust crate",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Name of the crate"
                                },
                                "limit": {
                                    "type": "integer",
                                    "description": "Maximum number of versions to return",
                                    "minimum": 1,
                                    "maximum": 50
                                }
                            },
                            "required": ["name"]
                        }
                    },
                    {
                        "name": "get_crate_dependencies",
                        "description": "Get dependencies for a specific version of a Rust crate",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Name of the crate"
                                },
                                "version": {
                                    "type": "string",
                                    "description": "Specific version (defaults to latest)"
                                }
                            },
                            "required": ["name"]
                        }
                    },
                    {
                        "name": "get_crate_documentation",
                        "description": "Get documentation information for a Rust crate from docs.rs",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Name of the crate"
                                },
                                "version": {
                                    "type": "string",
                                    "description": "Specific version (defaults to latest)"
                                }
                            },
                            "required": ["name"]
                        }
                    }
                ]
            }
        })
    }

    async fn handle_call_tool(
        &self,
        request: serde_json::Value,
        id: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let params = match request.get("params") {
            Some(p) => p,
            None => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": "Invalid params"
                    }
                });
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(name) => name,
            None => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": "Missing tool name"
                    }
                });
            }
        };

        let default_args = json!({});
        let arguments = params.get("arguments").unwrap_or(&default_args);

        debug!("Calling tool: {} with arguments: {}", tool_name, arguments);

        let result = match tool_name {
            "search_crates" => self.call_search_crates(arguments).await,
            "get_crate_info" => self.call_get_crate_info(arguments).await,
            "get_crate_versions" => self.call_get_crate_versions(arguments).await,
            "get_crate_dependencies" => self.call_get_crate_dependencies(arguments).await,
            "get_crate_documentation" => self.call_get_crate_documentation(arguments).await,
            _ => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": "Unknown tool"
                    }
                });
            }
        };

        match result {
            Ok(content) => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": content
                            }
                        ]
                    }
                })
            }
            Err(e) => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": format!("Error: {}", e)
                            }
                        ],
                        "isError": true
                    }
                })
            }
        }
    }

    async fn call_search_crates(&self, arguments: &serde_json::Value) -> Result<String> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .context("Missing 'query' parameter")?;

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        // Additional validation
        if let Some(limit) = limit {
            if limit > 100 {
                return Err(anyhow::anyhow!("Limit cannot exceed 100"));
            }
        }

        let results = self.crates_client.search_crates(query, limit).await?;
        serde_json::to_string_pretty(&results).context("Failed to serialize search results")
    }

    async fn call_get_crate_info(&self, arguments: &serde_json::Value) -> Result<String> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .context("Missing 'name' parameter")?;

        let info = self.crates_client.get_crate_info(name).await?;
        serde_json::to_string_pretty(&info).context("Failed to serialize crate info")
    }

    async fn call_get_crate_versions(&self, arguments: &serde_json::Value) -> Result<String> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .context("Missing 'name' parameter")?;

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let versions = self.crates_client.get_crate_versions(name, limit).await?;
        serde_json::to_string_pretty(&versions).context("Failed to serialize versions")
    }

    async fn call_get_crate_dependencies(&self, arguments: &serde_json::Value) -> Result<String> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .context("Missing 'name' parameter")?;

        let version = arguments.get("version").and_then(|v| v.as_str());

        let dependencies = self.crates_client.get_crate_dependencies(name, version)?;
        serde_json::to_string_pretty(&dependencies).context("Failed to serialize dependencies")
    }

    async fn call_get_crate_documentation(&self, arguments: &serde_json::Value) -> Result<String> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .context("Missing 'name' parameter")?;

        let version = arguments.get("version").and_then(|v| v.as_str());

        let docs = self
            .docs_client
            .get_crate_documentation(name, version)
            .await?;
        serde_json::to_string_pretty(&docs).context("Failed to serialize documentation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() -> Result<()> {
        let _server = CratesIoMcpServer::new().await?;
        Ok(())
    }
}
