# syntax=docker/dockerfile:1
# Сборка signaling-сервера. Контекст сборки — корень репозитория p2pchat/.
#   docker build -f deploy/Dockerfile -t p2pchat-signal .

FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY proto ./proto
COPY crates ./crates
RUN cargo build --release -p p2pchat-signal-server

FROM debian:bookworm-slim
RUN useradd -r -s /usr/sbin/nologin p2pchat
COPY --from=build /src/target/release/p2pchat-signal-server /usr/local/bin/p2pchat-signal-server
USER p2pchat
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/p2pchat-signal-server", "--listen", "0.0.0.0:8080"]
