FROM node:22-bookworm-slim AS frontend
WORKDIR /ui
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build && npm test

FROM rust:1.99.1-bookworm AS source
RUN rustup component add rustfmt clippy
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY tests/ ./tests/
COPY schema.sql openapi.json ./
COPY web/ ./web/
COPY --from=frontend /ui/dist ./web/dist
COPY scripts/ ./scripts/

FROM source AS verify
RUN cargo install cargo-audit --locked --version 0.22.2
CMD ["sh","scripts/check.sh"]

FROM source AS build
RUN cargo build --locked --release

FROM debian:bookworm-slim AS demo
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && useradd -u 10001 -m appuser
WORKDIR /app
COPY --from=build /app/target/release/importflow /app/importflow
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/app/importflow"]
