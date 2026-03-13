# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).


## [Unreleased]

## [0.2.2] - 2026-03-13

### Documentation
- Update README for stdio transport and improve PR template ([#10](https://github.com/davidlandais/ovh-api-mcp/pull/10))

## [0.2.1] - 2026-03-13

### Added
- Add stdio transport and cross-compiled binary releases

### Changed
- Deduplicate changelog config ([#7](https://github.com/davidlandais/ovh-api-mcp/pull/7))

### Fixed
- Clean changelog duplicates and auto-sync server.json ([#8](https://github.com/davidlandais/ovh-api-mcp/pull/8))
- Remove unsupported files field from release-plz config
- Correct release-plz.toml syntax for package files
- Allow server startup without OVH credentials

### Miscellaneous
- Allow manual trigger of release-plz workflow
- Add release-plz for automated release PRs and tagging
- Add git-cliff changelog generation and commit-msg hook
- Add CLAUDE.md and ignore PUBLICATION.md

## [0.2.0] - 2026-03-12

### Dependencies
- Upgrade secrecy 0.8→0.10, rust 1.92→1.94, alpine 3.21→3.23
- Upgrade rmcp 0.16 → 1.2

### Documentation
- Reframe as early release, add Dependabot
- Add Proof of Concept disclaimer
- Simplify OVH credentials to single createToken page
- Improve quick start and OVH credentials guide
- Add contribution guidelines, code of conduct, and issue templates
- Add publishing guide

### Fixed
- Correct Rust version badge to 1.92+

### Miscellaneous
- Prepare v0.2.0 release
- Add CI, release automation, SECURITY.md, CHANGELOG.md
- Add glama.json for server listing verification

### Style
- Apply rustfmt formatting

