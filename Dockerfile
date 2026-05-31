FROM rust:1.87-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p fenestra-cli

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false fenestra

COPY --from=builder /app/target/release/fenestra /usr/local/bin/fenestra

USER fenestra

ENV RUST_LOG=info,fenestra=debug
ENV FENESTRA_PORT=8080

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["fenestra"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8080"]
