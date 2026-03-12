# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-03-11

### Added

- Two MCP tools: `search` (explore OpenAPI spec) and `execute` (call OVH API)
- QuickJS sandbox with resource limits (64 MiB memory, 1 MiB stack, 10s/30s timeout)
- OpenAPI spec validation for every API call (`SpecValidator`)
- Path injection prevention (`?`, `#`, `..` rejected)
- Secret protection with `secrecy` crate (zeroize on drop)
- HTTP redirect blocking (credential leak prevention)
- Configurable services filter (`--services domain,email/domain`)
- Spec caching with configurable TTL
- Docker image (~19 MB, non-root)
- Published to MCP Registry as `io.github.davidlandais/ovh-api-mcp`

[0.1.0]: https://github.com/davidlandais/ovh-api-mcp/releases/tag/v0.1.0
