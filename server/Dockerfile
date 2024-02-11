# Builder
FROM rust:1.74-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

ADD tunnel_server/.cargo tunnel_server/.cargo
RUN mkdir -p tunnel_server/src && echo "fn main() {}" > tunnel_server/src/main.rs
RUN echo "[package]\nname = \"moosicbox_tunnel_server\"\nversion = \"0.1.0\"" > tunnel_server/Cargo.toml

ADD server/.cargo server/.cargo
RUN mkdir -p server/src && echo "fn main() {}" > server/src/main.rs
RUN echo "[package]\nname = \"moosicbox_server\"\nversion = \"0.1.0\"" > server/Cargo.toml

COPY packages packages
COPY players players

RUN cargo build --manifest-path=packages/env_utils/Cargo.toml --release
RUN cargo build --manifest-path=packages/database/Cargo.toml --release
RUN cargo build --manifest-path=packages/json_utils/Cargo.toml --release
RUN cargo build --manifest-path=packages/core/Cargo.toml --release
RUN cargo build --manifest-path=packages/auth/Cargo.toml --release
RUN cargo build --manifest-path=packages/image/Cargo.toml --release
RUN apt-get update && apt-get -y install libasound2-dev cmake
RUN cargo build --manifest-path=players/symphonia_player/Cargo.toml --release --no-default-features --features=cpal
RUN cargo build --manifest-path=packages/converter/Cargo.toml --release
RUN cargo build --manifest-path=packages/stream_utils/Cargo.toml --release
RUN cargo build --manifest-path=packages/paging/Cargo.toml --release
RUN cargo build --manifest-path=packages/music_api/Cargo.toml --release
RUN cargo build --manifest-path=packages/qobuz/Cargo.toml --release
RUN cargo build --manifest-path=packages/tidal/Cargo.toml --release
RUN cargo build --manifest-path=packages/config/Cargo.toml --release
RUN cargo build --manifest-path=packages/files/Cargo.toml --release
RUN cargo build --manifest-path=packages/downloader/Cargo.toml --release
RUN cargo build --manifest-path=packages/scan/Cargo.toml --release
RUN cargo build --manifest-path=packages/menu/Cargo.toml --release
RUN cargo build --manifest-path=packages/player/Cargo.toml --release --no-default-features --features=cpal
RUN cargo build --manifest-path=packages/ws/Cargo.toml --release
RUN cargo build --manifest-path=packages/tunnel/Cargo.toml --release --no-default-features --features=sender

COPY server server
ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
RUN cargo build --manifest-path=server/Cargo.toml --release --no-default-features --features=cpal,static-token-auth

RUN cargo install diesel_cli --no-default-features --features sqlite
COPY migrations/sqlite migrations/sqlite
RUN diesel migration run --migration-dir migrations/sqlite --database-url library.db

# Final
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl libasound2-dev sqlite3

COPY --from=builder /app/target/release/moosicbox_server /
COPY --from=builder /app/library.db /
EXPOSE 8010
ARG WS_HOST
ENV WS_HOST=${WS_HOST}
ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
CMD ["./moosicbox_server", "8010"]
