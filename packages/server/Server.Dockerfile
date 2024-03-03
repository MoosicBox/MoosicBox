# Builder
FROM rust:1.76-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/auth\",\r\
    \"packages\/config\",\r\
    \"packages\/converter\",\r\
    \"packages\/core\",\r\
    \"packages\/database\",\r\
    \"packages\/downloader\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/files\",\r\
    \"packages\/image\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/menu\",\r\
    \"packages\/music_api\",\r\
    \"packages\/paging\",\r\
    \"packages\/player\",\r\
    \"packages\/qobuz\",\r\
    \"packages\/scan\",\r\
    \"packages\/search\",\r\
    \"packages\/server\",\r\
    \"packages\/stream_utils\",\r\
    \"packages\/symphonia_player\",\r\
    \"packages\/tidal\",\r\
    \"packages\/tunnel\",\r\
    \"packages\/tunnel_sender\",\r\
    \"packages\/ws\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages packages

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -v -E "^(\
packages/auth|\
packages/config|\
packages/converter|\
packages/core|\
packages/database|\
packages/downloader|\
packages/env_utils|\
packages/files|\
packages/image|\
packages/json_utils|\
packages/menu|\
packages/music_api|\
packages/paging|\
packages/player|\
packages/qobuz|\
packages/scan|\
packages/search|\
packages/server|\
packages/stream_utils|\
packages/symphonia_player|\
packages/tidal|\
packages/tunnel|\
packages/tunnel_sender|\
packages/ws|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
RUN apt-get update && apt-get -y install libasound2-dev cmake
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,static-token-auth

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
