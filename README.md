# ovh-api-mcp

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2025--03--26-green.svg)](https://modelcontextprotocol.io/)

A native **Model Context Protocol (MCP)** server that gives LLMs full access to the **OVH API**. Built in Rust for minimal footprint (~19 MB Docker image, ~1.2 MiB RAM).

## How it works

The server exposes two MCP tools:

| Tool | Description |
|------|-------------|
| `search` | Explore the OVH OpenAPI spec using JavaScript — find endpoints, inspect schemas, read parameters |
| `execute` | Call any OVH API endpoint using JavaScript — authentication is handled transparently |

The LLM writes JavaScript that runs inside a **sandboxed QuickJS engine** with resource limits (memory, CPU timeout, stack size). Every API call is validated against the loaded OpenAPI spec before execution.

## Quick start

### From source

```bash
cargo install --path .

export OVH_APPLICATION_KEY=your_app_key
export OVH_APPLICATION_SECRET=your_app_secret
export OVH_CONSUMER_KEY=your_consumer_key

ovh-api-mcp --port 3104
```

### With Docker

```bash
docker build -t ovh-api-mcp .

docker run -d --name ovh-api \
  -e OVH_APPLICATION_KEY=your_app_key \
  -e OVH_APPLICATION_SECRET=your_app_secret \
  -e OVH_CONSUMER_KEY=your_consumer_key \
  -p 3104:3104 \
  ovh-api-mcp
```

### Claude Code configuration

Add to your MCP settings (`~/.claude/settings.json`):

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

Create your API credentials at:

| Endpoint | URL |
|----------|-----|
| Europe | https://eu.api.ovh.com/createApp/ |
| Canada | https://ca.api.ovh.com/createApp/ |
| US | https://api.us.ovhcloud.com/createApp/ |

Then request a consumer key:

```bash
curl -X POST https://eu.api.ovh.com/1.0/auth/credential \
  -H "X-Ovh-Application: YOUR_APP_KEY" \
  -H "Content-Type: application/json" \
  -d '{"accessRules": [{"method": "GET", "path": "/*"}, {"method": "POST", "path": "/*"}, {"method": "PUT", "path": "/*"}, {"method": "DELETE", "path": "/*"}]}'
```

## CLI options

```
Options:
  --port <PORT>                  Port to listen on [env: PORT] [default: 3104]
  --host <HOST>                  Host to bind to [default: 127.0.0.1]
  --endpoint <ENDPOINT>          OVH API endpoint: eu, ca, us [env: OVH_ENDPOINT] [default: eu]
  --app-key <APP_KEY>            OVH application key [env: OVH_APPLICATION_KEY]
  --app-secret <APP_SECRET>      OVH application secret [env: OVH_APPLICATION_SECRET]
  --consumer-key <CONSUMER_KEY>  OVH consumer key [env: OVH_CONSUMER_KEY]
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
async () => await ovh.request({ method: "GET", path: "/domain/zone" })
```

**Get DNS records for a domain:**
```javascript
// execute tool
async () => {
  const records = await ovh.request({
    method: "GET",
    path: "/domain/zone/example.com/record"
  });
  const details = [];
  for (const id of records.slice(0, 10)) {
    details.push(await ovh.request({
      method: "GET",
      path: `/domain/zone/example.com/record/${id}`
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
  main.rs      CLI, logging, axum server setup, graceful shutdown
  tools.rs     MCP tool definitions (search, execute) via rmcp macros
  sandbox.rs   QuickJS sandboxed JS execution with resource limits
  auth.rs      OVH API client with signature, clock sync, request handling
  spec.rs      OpenAPI spec fetching, caching, merging, and path validation
  types.rs     Input types for MCP tool parameters
```

## License

[MIT](LICENSE) — David Landais
