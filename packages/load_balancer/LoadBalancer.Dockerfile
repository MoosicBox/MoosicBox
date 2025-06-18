# Builder
FROM rust:1-bookworm AS builder
WORKDIR /app

# APT configuration for faster downloads
RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy

# Install basic build dependencies (early for better Docker layer caching)

RUN apt-get update && apt-get -y install cmake

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN sed -e '/^members = \[/,/^\]/c\members = ["packages/assert", "packages/config", "packages/env_utils", "packages/json_utils", "packages/logging", "packages/profiles", "packages/task", "packages/database", "packages/random", "packages/load_balancer"]' Cargo.toml > Cargo2.toml && mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/config/Cargo.toml packages/config/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/logging/Cargo.toml packages/logging/Cargo.toml
COPY packages/profiles/Cargo.toml packages/profiles/Cargo.toml
COPY packages/task/Cargo.toml packages/task/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/random/Cargo.toml packages/random/Cargo.toml
COPY packages/load_balancer/Cargo.toml packages/load_balancer/Cargo.toml

# Copy build.rs if it exists
RUN [ -f packages/load_balancer/build.rs ] && cp packages/load_balancer/build.rs packages/load_balancer/build.rs || true

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(packages/assert|packages/config|packages/env_utils|packages/json_utils|packages/logging|packages/profiles|packages/task|packages/database|packages/random)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

# Handle nested packages with correct lib paths

RUN mkdir -p packages/load_balancer/src && \
  echo 'fn main() {}' >packages/load_balancer/src/main.rs

# Environment setup
ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
ARG WS_HOST
ENV WS_HOST=${WS_HOST}
ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace

RUN cargo build --package moosicbox_load_balancer --release 

COPY packages packages

RUN rm -f target/release/deps/moosicbox_lb*
RUN cargo build --package moosicbox_load_balancer --release 

# Final
FROM debian:bookworm-slim

RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \

  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get install -y ca-certificates curl sqlite3
COPY --from=builder /app/target/release/moosicbox_lb /
EXPOSE 8011
ARG STATIC_TOKEN
ENV STATIC_TOKEN=${STATIC_TOKEN}
ARG WS_HOST
ENV WS_HOST=${WS_HOST}
ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
CMD ["./moosicbox_lb", "8011"]
