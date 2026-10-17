# Build the ChatBox backend and produce a slim runtime image.
#
# Build from the repository root:
#   docker build -f deploy/docker/Dockerfile -t chatbox-backend .

FROM rust:1.99-slim AS builder
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY backend/ ./backend/
RUN cargo build --release --locked -p chatbox-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/backend/target/release/chatbox-server /usr/local/bin/chatbox-server
WORKDIR /app
EXPOSE 8080
CMD ["chatbox-server"]
