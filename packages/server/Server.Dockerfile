# Builder
FROM rust:1.76-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/assert\",\r\
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
    \"packages\/upnp\",\r\
    \"packages\/ws\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/auth/Cargo.toml packages/auth/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/converter/Cargo.toml packages/converter/Cargo.toml
COPY packages/core/Cargo.toml packages/core/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/downloader/Cargo.toml packages/downloader/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/files/Cargo.toml packages/files/Cargo.toml
COPY packages/image/Cargo.toml packages/image/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/menu/Cargo.toml packages/menu/Cargo.toml
COPY packages/music_api/Cargo.toml packages/music_api/Cargo.toml
COPY packages/paging/Cargo.toml packages/paging/Cargo.toml
COPY packages/player/Cargo.toml packages/player/Cargo.toml
COPY packages/qobuz/Cargo.toml packages/qobuz/Cargo.toml
COPY packages/scan/Cargo.toml packages/scan/Cargo.toml
COPY packages/search/Cargo.toml packages/search/Cargo.toml
COPY packages/server/Cargo.toml packages/server/Cargo.toml
COPY packages/stream_utils/Cargo.toml packages/stream_utils/Cargo.toml
COPY packages/symphonia_player/Cargo.toml packages/symphonia_player/Cargo.toml
COPY packages/tidal/Cargo.toml packages/tidal/Cargo.toml
COPY packages/tunnel/Cargo.toml packages/tunnel/Cargo.toml
COPY packages/tunnel_sender/Cargo.toml packages/tunnel_sender/Cargo.toml
COPY packages/upnp/Cargo.toml packages/upnp/Cargo.toml
COPY packages/ws/Cargo.toml packages/ws/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/assert|\
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
packages/upnp|\
packages/ws|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

RUN mkdir packages/server/src && \
  echo 'fn main() {}' >packages/server/src/main.rs

ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get -y install libasound2-dev cmake
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,static-token-auth

COPY packages packages

RUN rm target/release/deps/moosicbox*
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,static-token-auth

RUN cargo install diesel_cli --no-default-features --features sqlite
COPY migrations/server/sqlite migrations/server/sqlite
RUN diesel migration run --migration-dir migrations/server/sqlite --database-url library.db

# Final
FROM debian:bookworm-slim

RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
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
