# Contributing to ovh-api-mcp

Thank you for your interest in contributing! This project is a thin MCP bridge over the OVH API — its scope is deliberately narrow. Please read these guidelines before opening an issue or pull request.

## Scope

This project wraps the OVH API via the Model Context Protocol. It does **not** implement OVH business logic.

**Accepted contributions:**

- Bug fixes
- Documentation improvements
- Security patches
- Test coverage improvements
- Dependency updates

**Not accepted:**

- New features or new MCP tools — the feature set follows OVH's own API evolution
- Changes to the sandboxing model without prior discussion
- Large refactors without an approved issue first

If you're unsure whether your change fits, **open an issue first** to discuss it.

## Getting started

### Prerequisites

- Rust 1.92+ (`rustup update`)
- Docker (optional, for container testing)
- OVH API credentials ([create here](https://eu.api.ovh.com/createApp/))

### Build and test

```bash
cargo build
cargo test
cargo clippy
```

All three must pass with **zero warnings** before submitting a PR.

## Pull request process

1. **One issue, one PR** — link your PR to an existing issue. If there's no issue, create one first.
2. **Fork and branch** — create a feature branch from `main` (e.g., `fix/path-validation`).
3. **Conventional commits** — use the [Conventional Commits](https://www.conventionalcommits.org/) format:
   - `fix: description` for bug fixes
   - `docs: description` for documentation
   - `test: description` for test additions
   - `chore: description` for dependency updates, CI, tooling
   - `security: description` for security patches
4. **Keep it small** — focused PRs are reviewed faster. One fix per PR.
5. **Tests required** — if your change touches Rust code, add or update tests.
6. **CI must pass** — `cargo test` and `cargo clippy` are enforced in CI. A failing CI means the PR will not be reviewed.

## Code style

- Rust edition 2021, target `rust-version = "1.92"`
- Error handling: `anyhow` in `main`, `CallToolResult::error(...)` in tool handlers
- No `unwrap()` in production code
- Follow existing patterns in the codebase — when in doubt, look at how similar code is structured

## Reporting security issues

**Do not open a public issue for security vulnerabilities.** Instead, email the maintainer directly or use GitHub's [private vulnerability reporting](https://github.com/davidlandais/ovh-api-mcp/security/advisories/new).

## Code of conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold it.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
