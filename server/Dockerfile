# Builder
FROM rust:1.74-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

ADD packages/auth/.cargo packages/auth/.cargo
RUN mkdir -p packages/auth/src && echo "fn main() {}" > packages/auth/src/main.rs
RUN echo "[package]\nname = \"moosicbox_auth\"\nversion = \"0.1.0\"" > packages/auth/Cargo.toml

ADD packages/converter/.cargo packages/converter/.cargo
RUN mkdir -p packages/converter/src && echo "fn main() {}" > packages/converter/src/main.rs
RUN echo "[package]\nname = \"moosicbox_converter\"\nversion = \"0.1.0\"" > packages/converter/Cargo.toml

ADD packages/core/.cargo packages/core/.cargo
RUN mkdir -p packages/core/src && echo "fn main() {}" > packages/core/src/main.rs
RUN echo "[package]\nname = \"moosicbox_core\"\nversion = \"0.1.0\"" > packages/core/Cargo.toml

ADD packages/files/.cargo packages/files/.cargo
RUN mkdir -p packages/files/src && echo "fn main() {}" > packages/files/src/main.rs
RUN echo "[package]\nname = \"moosicbox_files\"\nversion = \"0.1.0\"" > packages/files/Cargo.toml

ADD packages/image/.cargo packages/image/.cargo
RUN mkdir -p packages/image/src && echo "fn main() {}" > packages/image/src/main.rs
RUN echo "[package]\nname = \"moosicbox_image\"\nversion = \"0.1.0\"" > packages/image/Cargo.toml

ADD packages/menu/.cargo packages/menu/.cargo
RUN mkdir -p packages/menu/src && echo "fn main() {}" > packages/menu/src/main.rs
RUN echo "[package]\nname = \"moosicbox_menu\"\nversion = \"0.1.0\"" > packages/menu/Cargo.toml

ADD packages/player/.cargo packages/player/.cargo
RUN mkdir -p packages/player/src && echo "fn main() {}" > packages/player/src/main.rs
RUN echo "[package]\nname = \"moosicbox_player\"\nversion = \"0.1.0\"" > packages/player/Cargo.toml

ADD packages/tunnel/.cargo packages/tunnel/.cargo
RUN mkdir -p packages/tunnel/src && echo "fn main() {}" > packages/tunnel/src/main.rs
RUN echo "[package]\nname = \"moosicbox_tunnel\"\nversion = \"0.1.0\"" > packages/tunnel/Cargo.toml

ADD tunnel_server/.cargo tunnel_server/.cargo
RUN mkdir -p tunnel_server/src && echo "fn main() {}" > tunnel_server/src/main.rs
RUN echo "[package]\nname = \"moosicbox_tunnel_server\"\nversion = \"0.1.0\"" > tunnel_server/Cargo.toml

ADD packages/ws/.cargo packages/ws/.cargo
RUN mkdir -p packages/ws/src && echo "fn main() {}" > packages/ws/src/main.rs
RUN echo "[package]\nname = \"moosicbox_ws\"\nversion = \"0.1.0\"" > packages/ws/Cargo.toml

ADD packages/stream_utils/.cargo packages/stream_utils/.cargo
RUN mkdir -p packages/stream_utils/src && echo "fn main() {}" > packages/stream_utils/src/main.rs
RUN echo "[package]\nname = \"moosicbox_stream_utils\"\nversion = \"0.1.0\"" > packages/stream_utils/Cargo.toml

ADD players/symphonia_player/.cargo players/symphonia_player/.cargo
RUN mkdir -p players/symphonia_player/src && echo "fn main() {}" > players/symphonia_player/src/main.rs
RUN echo "[package]\nname = \"moosicbox_symphonia_player\"\nversion = \"0.1.0\"" > players/symphonia_player/Cargo.toml

ADD server/.cargo server/.cargo
RUN mkdir -p server/src && echo "fn main() {}" > server/src/main.rs
RUN echo "[package]\nname = \"moosicbox_server\"\nversion = \"0.1.0\"" > server/Cargo.toml

COPY packages/core packages/core
RUN cargo build --manifest-path=packages/core/Cargo.toml --release
RUN rm packages/core/src/main.rs

COPY packages/auth packages/auth
RUN cargo build --manifest-path=packages/auth/Cargo.toml --release
RUN rm packages/auth/src/main.rs

COPY packages/image packages/image
RUN cargo build --manifest-path=packages/image/Cargo.toml --release
RUN rm packages/image/src/main.rs

RUN apt-get update && apt-get -y install libasound2-dev cmake
COPY players/symphonia_player players/symphonia_player
RUN cargo build --manifest-path=players/symphonia_player/Cargo.toml --release --no-default-features --features=cpal
RUN rm players/symphonia_player/src/main.rs

COPY packages/converter packages/converter
RUN cargo build --manifest-path=packages/converter/Cargo.toml --release
RUN rm packages/converter/src/main.rs

COPY packages/stream_utils packages/stream_utils
RUN cargo build --manifest-path=packages/stream_utils/Cargo.toml --release
RUN rm packages/stream_utils/src/main.rs

COPY packages/files packages/files
RUN cargo build --manifest-path=packages/files/Cargo.toml --release
RUN rm packages/files/src/main.rs

COPY packages/menu packages/menu
RUN cargo build --manifest-path=packages/menu/Cargo.toml --release
RUN rm packages/menu/src/main.rs

COPY packages/player packages/player
RUN cargo build --manifest-path=packages/player/Cargo.toml --release --no-default-features --features=cpal
RUN rm packages/player/src/main.rs

COPY packages/ws packages/ws
RUN cargo build --manifest-path=packages/ws/Cargo.toml --release
RUN rm packages/ws/src/main.rs

COPY packages/tunnel packages/tunnel
RUN cargo build --manifest-path=packages/tunnel/Cargo.toml --release --no-default-features --features=sender
RUN rm packages/tunnel/src/main.rs

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
ENV ACTIX_WORKERS=128
CMD ["./moosicbox_server", "8010"]
