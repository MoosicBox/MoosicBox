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
    \"packages\/config\",\r\
    \"packages\/database\",\r\
    \"packages\/database_connection\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/fs\",\r\
    \"packages\/http\",\r\
    \"packages\/http\/models\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/load_balancer\",\r\
    \"packages\/logging\",\r\
    \"packages\/mdns\",\r\
    \"packages\/profiles\",\r\
    \"packages\/random\",\r\
    \"packages\/simvar\",\r\
    \"packages\/simvar\/harness\",\r\
    \"packages\/simvar\/utils\",\r\
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
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/database_connection/Cargo.toml packages/database_connection/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/fs/Cargo.toml packages/fs/Cargo.toml
COPY packages/http/Cargo.toml packages/http/Cargo.toml
COPY packages/http/models/Cargo.toml packages/http/models/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/load_balancer/Cargo.toml packages/load_balancer/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/mdns/Cargo.toml packages/mdns/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/random/Cargo.toml packages/random/Cargo.toml
COPY packages/simvar/Cargo.toml packages/simvar/Cargo.toml
COPY packages/simvar/harness/Cargo.toml packages/simvar/harness/Cargo.toml
COPY packages/simvar/utils/Cargo.toml packages/simvar/utils/Cargo.toml
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
packages/config|\
packages/database|\
packages/database_connection|\
packages/env_utils|\
packages/fs|\
packages/http|\
packages/json_utils|\
packages/load_balancer|\
packages/logging|\
packages/mdns|\
packages/profiles|\
packages/random|\
packages/simvar|\
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
    mv "packages/http/models/Cargo2.toml" "packages/http/models/Cargo.toml" && \
    mv "packages/simvar/harness/Cargo2.toml" "packages/simvar/harness/Cargo.toml" && \
    printf "\n\n[lib]\npath=\"../../../temp_lib.rs\"" >> "packages/simvar/utils/Cargo.toml"

RUN apt-get update && apt-get install -y cmake
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
