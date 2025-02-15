# Builder
FROM rust:1-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/admin_htmx\",\r\
    \"packages\/assert\",\r\
    \"packages\/async_service\",\r\
    \"packages\/audio_decoder\",\r\
    \"packages\/audio_encoder\",\r\
    \"packages\/audio_output\",\r\
    \"packages\/audio_zone\",\r\
    \"packages\/audio_zone\/models\",\r\
    \"packages\/auth\",\r\
    \"packages\/channel_utils\",\r\
    \"packages\/config\",\r\
    \"packages\/database\",\r\
    \"packages\/database_connection\",\r\
    \"packages\/downloader\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/files\",\r\
    \"packages\/image\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/library\",\r\
    \"packages\/library\/models\",\r\
    \"packages\/logging\",\r\
    \"packages\/mdns\",\r\
    \"packages\/menu\",\r\
    \"packages\/menu/models\",\r\
    \"packages\/middleware\",\r\
    \"packages\/music_api\",\r\
    \"packages\/openport\",\r\
    \"packages\/paging\",\r\
    \"packages\/player\",\r\
    \"packages\/profiles\",\r\
    \"packages\/qobuz\",\r\
    \"packages\/resampler\",\r\
    \"packages\/scan\",\r\
    \"packages\/schema\",\r\
    \"packages\/search\",\r\
    \"packages\/server\",\r\
    \"packages\/session\",\r\
    \"packages\/session\/models\",\r\
    \"packages\/stream_utils\",\r\
    \"packages\/task\",\r\
    \"packages\/tidal\",\r\
    \"packages\/tunnel\",\r\
    \"packages\/tunnel_sender\",\r\
    \"packages\/upnp\",\r\
    \"packages\/ws\",\r\
    \"packages\/yt\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/admin_htmx/Cargo.toml packages/admin_htmx/Cargo.toml
COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/async_service/Cargo.toml packages/async_service/Cargo.toml
COPY packages/audio_decoder/Cargo.toml packages/audio_decoder/Cargo.toml
COPY packages/audio_encoder/Cargo.toml packages/audio_encoder/Cargo.toml
COPY packages/audio_output/Cargo.toml packages/audio_output/Cargo.toml
COPY packages/audio_zone/Cargo.toml packages/audio_zone/Cargo.toml
COPY packages/audio_zone/models/Cargo.toml packages/audio_zone/models/Cargo.toml
COPY packages/auth/Cargo.toml packages/auth/Cargo.toml
COPY packages/channel_utils/Cargo.toml packages/channel_utils/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/database_connection/Cargo.toml packages/database_connection/Cargo.toml
COPY packages/downloader/Cargo.toml packages/downloader/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/files/Cargo.toml packages/files/Cargo.toml
COPY packages/image/Cargo.toml packages/image/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/library/Cargo.toml packages/library/Cargo.toml
COPY packages/library/models/Cargo.toml packages/library/models/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/mdns/Cargo.toml packages/mdns/Cargo.toml
COPY packages/menu/Cargo.toml packages/menu/Cargo.toml
COPY packages/menu/models/Cargo.toml packages/menu/models/Cargo.toml
COPY packages/middleware/Cargo.toml packages/middleware/Cargo.toml
COPY packages/music_api/Cargo.toml packages/music_api/Cargo.toml
COPY packages/openport/Cargo.toml packages/openport/Cargo.toml
COPY packages/paging/Cargo.toml packages/paging/Cargo.toml
COPY packages/player/Cargo.toml packages/player/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/qobuz/Cargo.toml packages/qobuz/Cargo.toml
COPY packages/resampler/Cargo.toml packages/resampler/Cargo.toml
COPY packages/scan/Cargo.toml packages/scan/Cargo.toml
COPY packages/schema/Cargo.toml packages/schema/Cargo.toml
COPY packages/search/Cargo.toml packages/search/Cargo.toml
COPY packages/server/Cargo.toml packages/server/Cargo.toml
COPY packages/session/Cargo.toml packages/session/Cargo.toml
COPY packages/session/models/Cargo.toml packages/session/models/Cargo.toml
COPY packages/stream_utils/Cargo.toml packages/stream_utils/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml
COPY packages/tidal/Cargo.toml packages/tidal/Cargo.toml
COPY packages/tunnel/Cargo.toml packages/tunnel/Cargo.toml
COPY packages/tunnel_sender/Cargo.toml packages/tunnel_sender/Cargo.toml
COPY packages/upnp/Cargo.toml packages/upnp/Cargo.toml
COPY packages/ws/Cargo.toml packages/ws/Cargo.toml
COPY packages/yt/Cargo.toml packages/yt/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/admin_htmx|\
packages/assert|\
packages/async_service|\
packages/audio_decoder|\
packages/audio_encoder|\
packages/audio_output|\
packages/audio_zone|\
packages/audio_zone/models|\
packages/auth|\
packages/channel_utils|\
packages/config|\
packages/database|\
packages/database_connection|\
packages/downloader|\
packages/env_utils|\
packages/files|\
packages/image|\
packages/json_utils|\
packages/library|\
packages/library/models|\
packages/logging|\
packages/mdns|\
packages/menu|\
packages/menu/models|\
packages/middleware|\
packages/music_api|\
packages/openport|\
packages/paging|\
packages/player|\
packages/profiles|\
packages/qobuz|\
packages/resampler|\
packages/scan|\
packages/schema|\
packages/search|\
packages/server|\
packages/session|\
packages/session/models|\
packages/stream_utils|\
packages/task|\
packages/tidal|\
packages/tunnel|\
packages/tunnel_sender|\
packages/upnp|\
packages/ws|\
packages/yt|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

RUN mkdir packages/server/src && \
  echo 'fn main() {}' >packages/server/src/main.rs

ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get -y install libasound2-dev cmake
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,flac,static-token-auth,all-apis

COPY packages packages

RUN rm target/release/deps/moosicbox*
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,flac,static-token-auth,all-apis

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
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
CMD ["./moosicbox_server", "8010"]
