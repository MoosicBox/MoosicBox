# Builder
FROM rust:1-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/assert\",\r\
    \"packages\/config\",\r\
    \"packages\/database\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/load_balancer\",\r\
    \"packages\/logging\",\r\
    \"packages\/profiles\",\r\
    \"packages\/task\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/load_balancer/Cargo.toml packages/load_balancer/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/assert|\
packages/config|\
packages/database|\
packages/env_utils|\
packages/json_utils|\
packages/load_balancer|\
packages/logging|\
packages/profiles|\
packages/task|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

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
