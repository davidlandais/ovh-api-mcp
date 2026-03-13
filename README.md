# ovh-api-mcp

[![CI](https://github.com/davidlandais/ovh-api-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/davidlandais/ovh-api-mcp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2025--03--26-green.svg)](https://modelcontextprotocol.io/)
[![Status](https://img.shields.io/badge/Status-Early%20Release-blue.svg)](#status)

A native **Model Context Protocol (MCP)** server that gives LLMs full access to the **OVH API (v1 and v2)**. Built in Rust for minimal footprint (~19 MB Docker image, ~1.2 MiB RAM).

> **Early Release** — Designed for **local development use**. Security hardening has been applied (sandboxed execution, spec validation, secret protection), but the server has not been battle-tested at scale. Do not expose it to the public internet. Feedback and bug reports are welcome.

<a href="https://glama.ai/mcp/servers/davidlandais/ovh-api-mcp">
  <img width="380" height="200" src="https://glama.ai/mcp/servers/davidlandais/ovh-api-mcp/badge" alt="ovh-api-mcp MCP server" />
</a>

## How it works

The server exposes two MCP tools:

| Tool | Description |
|------|-------------|
| `search` | Explore the OVH OpenAPI spec using JavaScript — find endpoints, inspect schemas, read parameters |
| `execute` | Call any OVH API endpoint using JavaScript — authentication is handled transparently |

The LLM writes JavaScript that runs inside a **sandboxed QuickJS engine** with resource limits (memory, CPU timeout, stack size). Every API call is validated against the loaded OpenAPI spec before execution.

The server supports two transport modes:
- **HTTP** (Streamable HTTP) — for web-based clients and Docker deployments
- **stdio** — for direct integration with Claude Desktop, Cursor, and MCP inspectors

OVH credentials are optional at startup: the server starts and exposes its tools even without API keys. Tools return a clear error when called without credentials.

## Quick start

### With stdio (Claude Desktop / Cursor)

Add to your MCP client configuration:

```json
{
  "mcpServers": {
    "ovh-api": {
      "command": "ovh-api-mcp",
      "args": ["--transport", "stdio"],
      "env": {
        "OVH_APPLICATION_KEY": "your_app_key",
        "OVH_APPLICATION_SECRET": "your_app_secret",
        "OVH_CONSUMER_KEY": "your_consumer_key"
      }
    }
  }
}
```

### With Docker

```bash
docker run -d --name ovh-api \
  -e OVH_APPLICATION_KEY=your_app_key \
  -e OVH_APPLICATION_SECRET=your_app_secret \
  -e OVH_CONSUMER_KEY=your_consumer_key \
  -p 3104:3104 \
  ghcr.io/davidlandais/ovh-api-mcp:latest
```

### From source

```bash
cargo install --git https://github.com/davidlandais/ovh-api-mcp

export OVH_APPLICATION_KEY=your_app_key
export OVH_APPLICATION_SECRET=your_app_secret
export OVH_CONSUMER_KEY=your_consumer_key

ovh-api-mcp --port 3104
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/davidlandais/ovh-api-mcp/releases) — available for macOS (x86_64, aarch64) and Linux (x86_64 musl).

### Claude Code configuration (HTTP mode)

```json
{
  "mcpServers": {
    "ovh-api": {
      "type": "http",
      "url": "http://localhost:3104/mcp",
      "headers": {
        "Authorization": "Bearer local"
      }
    }
  }
}
```

> The `Authorization` header is required to bypass Claude Code's OAuth discovery. See [claude-code#2831](https://github.com/anthropics/claude-code/issues/2831).

## OVH credentials

You need three values: an **application key**, an **application secret**, and a **consumer key**.

Go to the token creation page for your region, log in with your OVH account, set the permissions and validity, and you'll get all three keys at once:

| Region | URL |
|--------|-----|
| Europe | https://auth.eu.ovhcloud.com/api/createToken |
| Canada | https://auth.ca.ovhcloud.com/api/createToken |
| US | https://auth.us.ovhcloud.com/api/createToken |

For full API access, set all four methods (`GET`, `POST`, `PUT`, `DELETE`) with path `/*`.

### OAuth2 authentication (service accounts)

As an alternative to API keys, you can use OVH service accounts with OAuth2 client credentials:

| Variable | Description |
|----------|-------------|
| `OVH_CLIENT_ID` | Service account ID |
| `OVH_CLIENT_SECRET` | Service account secret |

Create a service account in the [OVH Manager](https://www.ovh.com/manager/) under IAM > Service Accounts. Configure an access policy to grant the required API permissions.

The server auto-detects the auth mode from environment variables. Do not set both API keys and OAuth2 credentials at the same time.

## CLI options

```
Options:
  --transport <TRANSPORT>        Transport mode: http, stdio [env: OVH_TRANSPORT] [default: http]
  --port <PORT>                  Port to listen on [env: PORT] [default: 3104]
  --host <HOST>                  Host to bind to [default: 127.0.0.1]
  --endpoint <ENDPOINT>          OVH API endpoint: eu, ca, us [env: OVH_ENDPOINT] [default: eu]
  --app-key <APP_KEY>            OVH application key [env: OVH_APPLICATION_KEY]
  --app-secret <APP_SECRET>      OVH application secret [env: OVH_APPLICATION_SECRET]
  --consumer-key <CONSUMER_KEY>  OVH consumer key [env: OVH_CONSUMER_KEY]
  --client-id <CLIENT_ID>        OVH OAuth2 client ID [env: OVH_CLIENT_ID]
  --client-secret <CLIENT_SECRET> OVH OAuth2 client secret [env: OVH_CLIENT_SECRET]
  --services <SERVICES>          Services to load, comma-separated or "*" [env: OVH_SERVICES] [default: *]
  --cache-dir <PATH>             Directory to cache the merged spec [env: OVH_CACHE_DIR]
  --cache-ttl <SECONDS>          Cache TTL in seconds, 0 to disable [env: OVH_CACHE_TTL] [default: 86400]
  --no-cache                     Disable spec caching entirely
  --max-code-size <BYTES>        Maximum code input size [env: OVH_MAX_CODE_SIZE] [default: 1048576]
```

## Usage examples

Once connected, the LLM can use the tools like this:

**Search for DNS endpoints:**
```javascript
// search tool
(spec) => {
  const results = [];
  for (const [path, methods] of Object.entries(spec.paths)) {
    if (path.includes("/domain/zone")) {
      for (const [method, op] of Object.entries(methods)) {
        results.push({ method: method.toUpperCase(), path, summary: op.summary });
      }
    }
  }
  return results;
}
```

**List your domain zones:**
```javascript
// execute tool
async () => await ovh.request({ method: "GET", path: "/v1/domain/zone" })
```

**Get DNS records for a domain:**
```javascript
// execute tool
async () => {
  const records = await ovh.request({
    method: "GET",
    path: "/v1/domain/zone/example.com/record"
  });
  const details = [];
  for (const id of records.slice(0, 10)) {
    details.push(await ovh.request({
      method: "GET",
      path: `/v1/domain/zone/example.com/record/${id}`
    }));
  }
  return details;
}
```

## Security

- **Sandboxed execution** — JavaScript runs in QuickJS with memory limit (64 MiB), stack limit (1 MiB), and execution timeout (10s for search, 30s for execute)
- **Spec-validated API calls** — every `ovh.request()` call is matched against the loaded OpenAPI spec; unknown endpoints or wrong HTTP methods are rejected
- **Path injection prevention** — API paths containing `?`, `#`, or `..` are rejected
- **Secret protection** — `app_secret` and `consumer_key` are stored using `secrecy` (zeroized on drop)
- **No HTTP redirects** — prevents credential leakage to third-party domains
- **Non-root container** — Docker image runs as unprivileged user

## Architecture

```
src/
  main.rs      CLI, logging, transport selection (HTTP/stdio), graceful shutdown
  tools.rs     MCP tool definitions (search, execute) via rmcp macros
  sandbox.rs   QuickJS sandboxed JS execution with resource limits
  auth.rs      OVH API client with signature, clock sync, request handling
  spec.rs      OpenAPI spec fetching, caching, merging, and path validation
  types.rs     Input types for MCP tool parameters
```

## License

[MIT](LICENSE) — David Landais
