# ovh-api-mcp

Native Rust MCP server for the OVH API. Uses the "Code Mode" pattern: 2 tools (`search` + `execute`), the LLM writes JavaScript executed in a QuickJS sandbox.

## Commands

```bash
cargo build                      # Build
cargo test                       # 12 tests (6 SpecValidator + 3 security + 3 sandbox)
cargo clippy --all-targets       # Lint (0 warnings required)
cargo fmt --check                # Formatting

docker build -t ovh-api-mcp .    # Docker image (~19 MB)

# Run locally (requires OVH env vars)
ovh-api-mcp --port 3104 --services domain
```

## Architecture

```
src/
  main.rs      CLI (clap), logging, axum server, graceful shutdown
  tools.rs     MCP tools (search, execute) — rmcp macros #[tool_router] / #[tool_handler]
  sandbox.rs   QuickJS sandbox with resource limits (64 MiB mem, 1 MiB stack, 10s/30s timeout)
  auth.rs      OVH API client (API key SHA1 signature, OAuth2 client credentials, clock sync, SecretString)
  spec.rs      OpenAPI spec fetching, caching, merging, and SpecValidator
  types.rs     MCP input types (JsonSchema)
```

## Conventions

- **Rust 1.92+**, edition 2021
- **rmcp 1.2** — `#[tool_router]` / `#[tool_handler]` / `#[tool(name = "...")]`
- **secrecy 0.10** — `SecretString` (not `Secret<String>`), `.expose_secret()` returns `&str`
- **Error handling**: `anyhow` in `main`, `CallToolResult::error(...)` in tool handlers
- **Commits**: conventional commits (`fix:`, `deps:`, `docs:`, `chore:`, `security:`, `style:`, `test:`)
- **CI**: cargo fmt + clippy + test + docker build on every push/PR
- **Release**: `git tag vX.Y.Z && git push origin vX.Y.Z` triggers the automated workflow

## Security

- QuickJS sandbox with CPU/memory/stack limits
- SpecValidator: every API call validated against the OpenAPI spec
- Path injection rejected (`?`, `#`, `..`)
- SecretString with zeroize on drop
- HTTP redirects disabled (credential leak prevention)
- Non-root Docker container (mcpuser)

## Environment

OVH API keys are loaded from system environment. No `.env` file in the repo.

```
OVH_APPLICATION_KEY=...
OVH_APPLICATION_SECRET=...
OVH_CONSUMER_KEY=...
OVH_ENDPOINT=eu           # eu, ca, us
OVH_SERVICES=*             # or domain,email/domain,...
OVH_CLIENT_ID=...             # OAuth2 alternative (service account, created via API)
OVH_CLIENT_SECRET=...         # OAuth2 alternative (service account, created via API)
```

## Publication

See `PUBLICATION.md` (not versioned, in .gitignore) for:
- Published artifacts status (GitHub, ghcr.io, MCP Registry)
- Directory submissions status (awesome-mcp-servers, mcp.so, PulseMCP, Glama)
- Release and MCP Registry publication process
- Known traps (ghcr.io private by default, GitHub scopes, etc.)
