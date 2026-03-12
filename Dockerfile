FROM rust:1.94-alpine AS builder
RUN apk add --no-cache musl-dev pkgconf
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs \
    && touch src/auth.rs src/sandbox.rs src/spec.rs src/tools.rs src/types.rs
RUN cargo build --release 2>/dev/null || true
COPY src/ src/
RUN find src -name '*.rs' -exec touch {} + && cargo build --release

FROM alpine:3.21
RUN apk add --no-cache ca-certificates && adduser -D -H -s /sbin/nologin mcpuser
COPY --from=builder /app/target/release/ovh-api-mcp /usr/local/bin/
USER mcpuser
EXPOSE 3104
ENTRYPOINT ["ovh-api-mcp"]
CMD ["--host", "0.0.0.0"]
