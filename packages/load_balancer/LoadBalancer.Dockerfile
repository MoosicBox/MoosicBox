# Builder
FROM rust:1-bookworm as builder
WORKDIR /app

RUN apt-get update && apt-get install -y cmake

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/assert\",\r\
    \"packages\/async\",\r\
    \"packages\/async\/macros\",\r\
    \"packages\/async_service\",\r\
    \"packages\/audio_decoder\",\r\
    \"packages\/audio_encoder\",\r\
    \"packages\/audio_output\",\r\
    \"packages\/audio_zone\",\r\
    \"packages\/audio_zone\/models\",\r\
    \"packages\/auth\",\r\
    \"packages\/config\",\r\
    \"packages\/database\",\r\
    \"packages\/database_connection\",\r\
    \"packages\/date_utils\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/files\",\r\
    \"packages\/fs\",\r\
    \"packages\/http\",\r\
    \"packages\/http\/models\",\r\
    \"packages\/image\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/library\",\r\
    \"packages\/library\/models\",\r\
    \"packages\/load_balancer\",\r\
    \"packages\/logging\",\r\
    \"packages\/mdns\",\r\
    \"packages\/menu\/models\",\r\
    \"packages\/middleware\",\r\
    \"packages\/music\/models\",\r\
    \"packages\/music_api\",\r\
    \"packages\/music_api\/api\",\r\
    \"packages\/music_api\/helpers\",\r\
    \"packages\/music_api\/models\",\r\
    \"packages\/paging\",\r\
    \"packages\/parsing_utils\",\r\
    \"packages\/player\",\r\
    \"packages\/profiles\",\r\
    \"packages\/random\",\r\
    \"packages\/resampler\",\r\
    \"packages\/scan\",\r\
    \"packages\/search\",\r\
    \"packages\/session\",\r\
    \"packages\/session\/models\",\r\
    \"packages\/simvar\",\r\
    \"packages\/simvar\/harness\",\r\
    \"packages\/simvar\/utils\",\r\
    \"packages\/stream_utils\",\r\
    \"packages\/switchy\",\r\
    \"packages\/task\",\r\
    \"packages\/tcp\",\r\
    \"packages\/telemetry\",\r\
    \"packages\/time\",\r\
    \"packages\/upnp\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/async/Cargo.toml packages/async/Cargo.toml
COPY packages/async/macros/Cargo.toml packages/async/macros/Cargo.toml
COPY packages/async_service/Cargo.toml packages/async_service/Cargo.toml
COPY packages/audio_decoder/Cargo.toml packages/audio_decoder/Cargo.toml
COPY packages/audio_encoder/Cargo.toml packages/audio_encoder/Cargo.toml
COPY packages/audio_output/Cargo.toml packages/audio_output/Cargo.toml
COPY packages/audio_zone/Cargo.toml packages/audio_zone/Cargo.toml
COPY packages/audio_zone/models/Cargo.toml packages/audio_zone/models/Cargo.toml
COPY packages/auth/Cargo.toml packages/auth/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/database_connection/Cargo.toml packages/database_connection/Cargo.toml
COPY packages/date_utils/Cargo.toml packages/date_utils/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/files/Cargo.toml packages/files/Cargo.toml
COPY packages/fs/Cargo.toml packages/fs/Cargo.toml
COPY packages/http/Cargo.toml packages/http/Cargo.toml
COPY packages/http/models/Cargo.toml packages/http/models/Cargo.toml
COPY packages/image/Cargo.toml packages/image/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/library/Cargo.toml packages/library/Cargo.toml
COPY packages/library/models/Cargo.toml packages/library/models/Cargo.toml
COPY packages/load_balancer/Cargo.toml packages/load_balancer/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/mdns/Cargo.toml packages/mdns/Cargo.toml
COPY packages/menu/models/Cargo.toml packages/menu/models/Cargo.toml
COPY packages/middleware/Cargo.toml packages/middleware/Cargo.toml
COPY packages/music/models/Cargo.toml packages/music/models/Cargo.toml
COPY packages/music_api/Cargo.toml packages/music_api/Cargo.toml
COPY packages/music_api/api/Cargo.toml packages/music_api/api/Cargo.toml
COPY packages/music_api/helpers/Cargo.toml packages/music_api/helpers/Cargo.toml
COPY packages/music_api/models/Cargo.toml packages/music_api/models/Cargo.toml
COPY packages/paging/Cargo.toml packages/paging/Cargo.toml
COPY packages/parsing_utils/Cargo.toml packages/parsing_utils/Cargo.toml
COPY packages/player/Cargo.toml packages/player/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/random/Cargo.toml packages/random/Cargo.toml
COPY packages/resampler/Cargo.toml packages/resampler/Cargo.toml
COPY packages/scan/Cargo.toml packages/scan/Cargo.toml
COPY packages/search/Cargo.toml packages/search/Cargo.toml
COPY packages/session/Cargo.toml packages/session/Cargo.toml
COPY packages/session/models/Cargo.toml packages/session/models/Cargo.toml
COPY packages/simvar/Cargo.toml packages/simvar/Cargo.toml
COPY packages/simvar/harness/Cargo.toml packages/simvar/harness/Cargo.toml
COPY packages/simvar/utils/Cargo.toml packages/simvar/utils/Cargo.toml
COPY packages/stream_utils/Cargo.toml packages/stream_utils/Cargo.toml
COPY packages/switchy/Cargo.toml packages/switchy/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml
COPY packages/tcp/Cargo.toml packages/tcp/Cargo.toml
COPY packages/telemetry/Cargo.toml packages/telemetry/Cargo.toml
COPY packages/time/Cargo.toml packages/time/Cargo.toml
COPY packages/upnp/Cargo.toml packages/upnp/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/assert|\
packages/async|\
packages/async_service|\
packages/audio_decoder|\
packages/audio_encoder|\
packages/audio_output|\
packages/audio_zone|\
packages/auth|\
packages/config|\
packages/database|\
packages/database_connection|\
packages/date_utils|\
packages/env_utils|\
packages/files|\
packages/fs|\
packages/http|\
packages/image|\
packages/json_utils|\
packages/library|\
packages/load_balancer|\
packages/logging|\
packages/mdns|\
packages/middleware|\
packages/music_api|\
packages/paging|\
packages/parsing_utils|\
packages/player|\
packages/profiles|\
packages/random|\
packages/resampler|\
packages/scan|\
packages/search|\
packages/session|\
packages/simvar|\
packages/stream_utils|\
packages/switchy|\
packages/task|\
packages/tcp|\
packages/telemetry|\
packages/time|\
packages/upnp|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

RUN \
    cat "packages/async/macros/Cargo.toml" | \
    tr '\n' '\r' | \
    sed -E "s/\[lib\]/[lib]\r\
path=\"..\/..\/temp_lib.rs\"/" | \
    tr '\r' '\n' \
    > "packages/async/macros/Cargo2.toml" && \
    mv "packages/async/macros/Cargo2.toml" "packages/async/macros/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/audio_zone/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/http/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/library/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/menu/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/api/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/helpers/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/music_api/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/session/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/simvar/harness/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/simvar/utils/Cargo.toml"

RUN mkdir packages/load_balancer/src && \
  echo 'fn main() {}' >packages/load_balancer/src/main.rs

ENV RUST_LOG=info,moosicbox=debug
RUN cargo build --package moosicbox_load_balancer --release --no-default-features

COPY packages packages

RUN rm target/release/deps/moosicbox*
RUN cargo build --package moosicbox_load_balancer --release --no-default-features

# Final
FROM debian:bookworm-slim

COPY --from=builder /app/target/release/moosicbox_lb /
EXPOSE 8007
ENV RUST_LOG=info,moosicbox=debug
ARG CLUSTERS
ENV CLUSTERS=${CLUSTERS}
ENV PORT=8007
ENV SSL_PORT=8008
CMD ["./moosicbox_lb"]
