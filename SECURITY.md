# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.**

Please report security issues through GitHub's private vulnerability reporting:

https://github.com/davidlandais/ovh-api-mcp/security/advisories/new

You will receive an acknowledgment within 48 hours and a detailed response within 7 days.

## Security measures

This project applies the following hardening:

- Sandboxed JavaScript execution (QuickJS) with memory, stack, and CPU limits
- Every API call validated against the loaded OpenAPI spec
- Path injection prevention (`?`, `#`, `..` rejected)
- Secrets stored with `secrecy` (zeroized on drop)
- HTTP redirects disabled (credential leak prevention)
- Docker image runs as non-root user
