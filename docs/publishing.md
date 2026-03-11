# Publishing guide — ovh-api-mcp

How to publish a new version of ovh-api-mcp.

## Prerequisites

- `mcp-publisher` CLI: `brew install mcp-publisher`
- Docker logged into ghcr.io: `gh auth token | docker login ghcr.io -u davidlandais --password-stdin`
- GitHub auth with `write:packages` scope: `gh auth refresh -h github.com -s write:packages,read:packages`

## Step-by-step release process

### 1. Bump version

Update the version in these files:
- `Cargo.toml` → `version = "X.Y.Z"`
- `server.json` → `"version": "X.Y.Z"` and `"identifier": "ghcr.io/davidlandais/ovh-api-mcp:X.Y.Z"`

### 2. Build and push Docker image

```bash
docker build \
  --label "org.opencontainers.image.source=https://github.com/davidlandais/ovh-api-mcp" \
  --label "org.opencontainers.image.description=MCP server for the OVH API" \
  --label "org.opencontainers.image.licenses=MIT" \
  --label "io.modelcontextprotocol.server.name=io.github.davidlandais/ovh-api-mcp" \
  -t ghcr.io/davidlandais/ovh-api-mcp:X.Y.Z \
  -t ghcr.io/davidlandais/ovh-api-mcp:latest \
  .

docker push ghcr.io/davidlandais/ovh-api-mcp:X.Y.Z
docker push ghcr.io/davidlandais/ovh-api-mcp:latest
```

### 3. Commit, tag, push

```bash
git add -A
git commit -m "release: vX.Y.Z"
git push origin main
```

### 4. Create GitHub Release

```bash
gh release create vX.Y.Z --title "vX.Y.Z" --notes "Release notes here"
```

### 5. Publish to MCP Registry

```bash
mcp-publisher login github    # only needed once
mcp-publisher publish
```

Verify:
```bash
curl -s 'https://registry.modelcontextprotocol.io/v0.1/servers?search=ovh-api' | python3 -m json.tool
```

## Infrastructure

| Service | URL |
|---------|-----|
| GitHub repo | https://github.com/davidlandais/ovh-api-mcp |
| Docker image | ghcr.io/davidlandais/ovh-api-mcp |
| MCP Registry | https://registry.modelcontextprotocol.io (name: `io.github.davidlandais/ovh-api-mcp`) |
| GitHub Release | https://github.com/davidlandais/ovh-api-mcp/releases |

## Container package visibility

The ghcr.io package must be **public** for anonymous pulls and MCP Registry validation.
To change visibility: https://github.com/users/davidlandais/packages/container/package/ovh-api-mcp/settings → Danger Zone → Change visibility.

## MCP Registry schema

The `server.json` follows schema `2025-12-11`. Key rules:
- `registryType`: `"oci"` for Docker images
- `identifier`: full reference like `ghcr.io/davidlandais/ovh-api-mcp:0.1.0` (no separate `registryBaseUrl`)
- `transport.type`: `"stdio"` for container-based distribution
- `description`: max 100 characters
- `name`: must be `io.github.davidlandais/ovh-api-mcp` (matches GitHub namespace)
