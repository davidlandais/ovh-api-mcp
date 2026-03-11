---
name: Bug report
about: Report a reproducible bug
title: "fix: "
labels: bug
---

<!--
BEFORE OPENING THIS ISSUE:
- Make sure you can reproduce the bug locally
- Make sure you are using the latest version
- Search existing issues to avoid duplicates
-->

## Version

<!-- Output of: ovh-api-mcp --version (or Docker image tag) -->

```
```

## Environment

- **OS**: <!-- e.g., macOS 14.5, Ubuntu 24.04 -->
- **Rust version** (if built from source): <!-- rustc --version -->
- **Installation method**: Docker / cargo install / from source
- **MCP client**: Claude Code / Cursor / other (version)
- **OVH endpoint**: eu / ca / us
- **OVH services**: <!-- e.g., domain,email/domain or * -->

## Reproduction steps

<!--
REQUIRED. Issues without clear reproduction steps will be closed.
Provide the exact commands to reproduce the bug from a clean state.
-->

```bash
# 1. Start the server
ovh-api-mcp --port 3104 --services domain

# 2. Send this MCP tool call (search or execute)
# Paste the JavaScript code you used

# 3. Observe the error
```

## Expected behavior

<!-- What should happen. -->

## Actual behavior

<!-- What actually happens. Include the full error message. -->

## Server logs

<!--
REQUIRED. Start the server with RUST_LOG=debug and paste the relevant output.
-->

<details>
<summary>Logs</summary>

```
RUST_LOG=debug ovh-api-mcp --port 3104 --services domain 2>&1 | head -50
```

</details>

## Additional context

<!-- Optional: screenshots, related issues, workaround you found. -->
