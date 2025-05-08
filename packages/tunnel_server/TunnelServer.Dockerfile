# Builder
FROM rust:1-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/assert\",\r\
    \"packages\/async\",\r\
    \"packages\/async\/macros\",\r\
    \"packages\/async_service\",\r\
    \"packages\/config\",\r\
    \"packages\/database\",\r\
    \"packages\/database_connection\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/fs\",\r\
    \"packages\/http\",\r\
    \"packages\/http\/models\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/logging\",\r\
    \"packages\/mdns\",\r\
    \"packages\/middleware\",\r\
    \"packages\/profiles\",\r\
    \"packages\/random\",\r\
    \"packages\/simulator\/utils\",\r\
    \"packages\/switchy\",\r\
    \"packages\/task\",\r\
    \"packages\/tcp\",\r\
    \"packages\/telemetry\",\r\
    \"packages\/time\",\r\
    \"packages\/tunnel\",\r\
    \"packages\/tunnel_server\",\r\
    \"packages\/upnp\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/async/Cargo.toml packages/async/Cargo.toml
COPY packages/async/macros/Cargo.toml packages/async/macros/Cargo.toml
COPY packages/async_service/Cargo.toml packages/async_service/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/database_connection/Cargo.toml packages/database_connection/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/fs/Cargo.toml packages/fs/Cargo.toml
COPY packages/http/Cargo.toml packages/http/Cargo.toml
COPY packages/http/models/Cargo.toml packages/http/models/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/mdns/Cargo.toml packages/mdns/Cargo.toml
COPY packages/middleware/Cargo.toml packages/middleware/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/random/Cargo.toml packages/random/Cargo.toml
COPY packages/simulator/utils/Cargo.toml packages/simulator/utils/Cargo.toml
COPY packages/switchy/Cargo.toml packages/switchy/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml
COPY packages/tcp/Cargo.toml packages/tcp/Cargo.toml
COPY packages/telemetry/Cargo.toml packages/telemetry/Cargo.toml
COPY packages/time/Cargo.toml packages/time/Cargo.toml
COPY packages/tunnel/Cargo.toml packages/tunnel/Cargo.toml
COPY packages/tunnel_server/Cargo.toml packages/tunnel_server/Cargo.toml
COPY packages/tunnel_server/build.rs packages/tunnel_server/build.rs
COPY packages/upnp/Cargo.toml packages/upnp/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/assert|\
packages/async|\
packages/async_service|\
packages/config|\
packages/database|\
packages/database_connection|\
packages/env_utils|\
packages/fs|\
packages/http|\
packages/json_utils|\
packages/logging|\
packages/mdns|\
packages/middleware|\
packages/profiles|\
packages/random|\
packages/switchy|\
packages/task|\
packages/tcp|\
packages/telemetry|\
packages/time|\
packages/tunnel|\
packages/tunnel_server|\
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
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/http/models/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/simulator/utils/Cargo.toml"

RUN mkdir packages/tunnel_server/src && \
  echo 'fn main() {}' >packages/tunnel_server/src/main.rs

ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
RUN cargo build --package moosicbox_tunnel_server --release --no-default-features --features postgres-raw,postgres-native-tls

COPY packages packages

RUN rm target/release/deps/moosicbox*
RUN cargo build --package moosicbox_tunnel_server --release --no-default-features --features postgres-raw,postgres-native-tls

# Final
FROM debian:bookworm-slim

RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get install -y ca-certificates curl

COPY --from=builder /app/target/release/moosicbox_tunnel_server /
EXPOSE 8004
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
ARG AWS_ACCESS_KEY_ID
ENV AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}
ARG AWS_SECRET_ACCESS_KEY
ENV AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}
CMD ["./moosicbox_tunnel_server", "8004"]
