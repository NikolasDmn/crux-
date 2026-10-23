# Build context: the repo root (core compiles contracts/ in). See deploy/compose.yaml.
FROM rust:1.91-bookworm AS build
WORKDIR /src
COPY contracts ./contracts
COPY core ./core
WORKDIR /src/core
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/core/target \
    cargo build --release --bin crux-core && cp target/release/crux-core /crux-core

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 crux
WORKDIR /app
COPY --from=build /crux-core /usr/local/bin/crux-core
COPY core/data ./data
USER crux
EXPOSE 8080
CMD ["crux-core", "serve"]
