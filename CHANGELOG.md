# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).


## [Unreleased]

### Added
- Add stdio transport and cross-compiled binary releases

### Fixed
- Allow server startup without OVH credentials

### Miscellaneous
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

