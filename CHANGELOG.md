# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-03-12

### Changed

- Upgrade rmcp 0.16 → 1.2 (builder pattern for ServerInfo/Implementation)
- Upgrade secrecy 0.8 → 0.10 (Secret\<String\> → SecretString)
- Upgrade Docker base images: rust 1.92 → 1.94, alpine 3.21 → 3.23
- Reframe project status from "Proof of Concept" to "Early Release (v0.1)"
- Bump MSRV from 1.85 to 1.92
- GitHub Actions: checkout@v4 → v5

### Added

- CI workflow: cargo fmt, clippy, test, Docker build on every push/PR
- Release workflow: automated Docker push + GitHub Release on tag
- Dependabot: weekly Cargo + Docker base image updates
- SECURITY.md: security policy with private vulnerability reporting
- CHANGELOG.md
- Glama badge in README

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

[0.2.0]: https://github.com/davidlandais/ovh-api-mcp/releases/tag/v0.2.0
[0.1.0]: https://github.com/davidlandais/ovh-api-mcp/releases/tag/v0.1.0
