# Builder
FROM rust:1-bookworm AS builder
WORKDIR /app

# APT configuration for faster downloads
RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy

# Install system dependencies (early for better Docker layer caching)
RUN apt-get update && \
    apt-get -y install libasound2-dev libgl1-mesa-dev libglu1-mesa-dev libpango1.0-dev libx11-dev libxcursor-dev libxext-dev libxfixes-dev libxft-dev libxinerama-dev libxrender-dev


COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN sed -e '/^members = \[/,/^\]/c\members = ["packages/admin_htmx", "packages/assert", "packages/async_service", "packages/audio_decoder", "packages/audio_encoder", "packages/audio_output", "packages/audio_zone", "packages/audio_zone/models", "packages/auth", "packages/channel_utils", "packages/config", "packages/date_utils", "packages/downloader", "packages/env_utils", "packages/files", "packages/image", "packages/json_utils", "packages/library", "packages/library/models", "packages/library/music_api", "packages/logging", "packages/menu", "packages/menu/models", "packages/middleware", "packages/music_api", "packages/music_api/api", "packages/music_api/helpers", "packages/music_api/models", "packages/music/models", "packages/paging", "packages/parsing_utils", "packages/player", "packages/profiles", "packages/qobuz", "packages/remote_library", "packages/resampler", "packages/scan", "packages/scan/models", "packages/schema", "packages/search", "packages/session", "packages/session/models", "packages/stream_utils", "packages/task", "packages/tidal", "packages/tunnel", "packages/tunnel_sender", "packages/ws", "packages/yt", "packages/switchy", "packages/async", "packages/async/macros", "packages/database", "packages/database_connection", "packages/fs", "packages/http", "packages/http/models", "packages/mdns", "packages/random", "packages/tcp", "packages/telemetry", "packages/time", "packages/upnp", "packages/server"]' Cargo.toml > Cargo2.toml && mv Cargo2.toml Cargo.toml

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
COPY packages/date_utils/Cargo.toml packages/date_utils/Cargo.toml
COPY packages/downloader/Cargo.toml packages/downloader/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/files/Cargo.toml packages/files/Cargo.toml
COPY packages/image/Cargo.toml packages/image/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/library/Cargo.toml packages/library/Cargo.toml
COPY packages/library/models/Cargo.toml packages/library/models/Cargo.toml
COPY packages/library/music_api/Cargo.toml packages/library/music_api/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/menu/Cargo.toml packages/menu/Cargo.toml
COPY packages/menu/models/Cargo.toml packages/menu/models/Cargo.toml
COPY packages/middleware/Cargo.toml packages/middleware/Cargo.toml
COPY packages/music_api/Cargo.toml packages/music_api/Cargo.toml
COPY packages/music_api/api/Cargo.toml packages/music_api/api/Cargo.toml
COPY packages/music_api/helpers/Cargo.toml packages/music_api/helpers/Cargo.toml
COPY packages/music_api/models/Cargo.toml packages/music_api/models/Cargo.toml
COPY packages/music/models/Cargo.toml packages/music/models/Cargo.toml
COPY packages/paging/Cargo.toml packages/paging/Cargo.toml
COPY packages/parsing_utils/Cargo.toml packages/parsing_utils/Cargo.toml
COPY packages/player/Cargo.toml packages/player/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/qobuz/Cargo.toml packages/qobuz/Cargo.toml
COPY packages/remote_library/Cargo.toml packages/remote_library/Cargo.toml
COPY packages/resampler/Cargo.toml packages/resampler/Cargo.toml
COPY packages/scan/Cargo.toml packages/scan/Cargo.toml
COPY packages/scan/models/Cargo.toml packages/scan/models/Cargo.toml
COPY packages/schema/Cargo.toml packages/schema/Cargo.toml
COPY packages/search/Cargo.toml packages/search/Cargo.toml
COPY packages/session/Cargo.toml packages/session/Cargo.toml
COPY packages/session/models/Cargo.toml packages/session/models/Cargo.toml
COPY packages/stream_utils/Cargo.toml packages/stream_utils/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml
COPY packages/tidal/Cargo.toml packages/tidal/Cargo.toml
COPY packages/tunnel/Cargo.toml packages/tunnel/Cargo.toml
COPY packages/tunnel_sender/Cargo.toml packages/tunnel_sender/Cargo.toml
COPY packages/ws/Cargo.toml packages/ws/Cargo.toml
COPY packages/yt/Cargo.toml packages/yt/Cargo.toml
COPY packages/switchy/Cargo.toml packages/switchy/Cargo.toml
COPY packages/async/Cargo.toml packages/async/Cargo.toml
COPY packages/async/macros/Cargo.toml packages/async/macros/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/database_connection/Cargo.toml packages/database_connection/Cargo.toml
COPY packages/fs/Cargo.toml packages/fs/Cargo.toml
COPY packages/http/Cargo.toml packages/http/Cargo.toml
COPY packages/http/models/Cargo.toml packages/http/models/Cargo.toml
COPY packages/mdns/Cargo.toml packages/mdns/Cargo.toml
COPY packages/random/Cargo.toml packages/random/Cargo.toml
COPY packages/tcp/Cargo.toml packages/tcp/Cargo.toml
COPY packages/telemetry/Cargo.toml packages/telemetry/Cargo.toml
COPY packages/time/Cargo.toml packages/time/Cargo.toml
COPY packages/upnp/Cargo.toml packages/upnp/Cargo.toml
COPY packages/server/Cargo.toml packages/server/Cargo.toml

# Copy build.rs if it exists
RUN [ -f packages/server/build.rs ] && cp packages/server/build.rs packages/server/build.rs || true

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(packages/admin_htmx|packages/assert|packages/async_service|packages/audio_decoder|packages/audio_encoder|packages/audio_output|packages/audio_zone|packages/audio_zone/models|packages/auth|packages/channel_utils|packages/config|packages/date_utils|packages/downloader|packages/env_utils|packages/files|packages/image|packages/json_utils|packages/library|packages/library/models|packages/library/music_api|packages/logging|packages/menu|packages/menu/models|packages/middleware|packages/music_api|packages/music_api/api|packages/music_api/helpers|packages/music_api/models|packages/music/models|packages/paging|packages/parsing_utils|packages/player|packages/profiles|packages/qobuz|packages/remote_library|packages/resampler|packages/scan|packages/scan/models|packages/schema|packages/search|packages/session|packages/session/models|packages/stream_utils|packages/task|packages/tidal|packages/tunnel|packages/tunnel_sender|packages/ws|packages/yt|packages/switchy|packages/async|packages/async/macros|packages/database|packages/database_connection|packages/fs|packages/http|packages/http/models|packages/mdns|packages/random|packages/tcp|packages/telemetry|packages/time|packages/upnp)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

# Handle nested packages with correct lib paths
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/audio_zone/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/library/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/library/music_api/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/menu/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/api/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/helpers/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/scan/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/session/models/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/async/macros/Cargo.toml"
RUN printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/http/models/Cargo.toml"

RUN mkdir -p packages/server/src && \
  echo 'fn main() {}' >packages/server/src/main.rs

# Environment setup
ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
ARG WS_HOST
ENV WS_HOST=${WS_HOST}
ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace

RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,format-flac,static-token-auth,all-apis

COPY packages packages

RUN rm -f target/release/deps/moosicbox_server*
RUN cargo build --package moosicbox_server --release --no-default-features --features=cpal,format-flac,static-token-auth,all-apis

# Final
FROM debian:bookworm-slim

RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \

  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get install -y ca-certificates curl libasound2 sqlite3
COPY --from=builder /app/target/release/moosicbox_server /
EXPOSE 8010
ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
ARG WS_HOST
ENV WS_HOST=${WS_HOST}
ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
CMD ["./moosicbox_server", "8010"]
