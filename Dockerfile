FROM rust:1.75-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev
COPY . .
RUN cargo build --release

FROM ubuntu:22.04
RUN apt-get update && apt-get install -y \
    chromium-browser \
    ca-certificates \
    libssl3 \
    --no-install-recommends \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_EXECUTABLE=/usr/bin/chromium-browser

COPY --from=builder /app/target/release/apollos-ui-navigator /usr/local/bin/
EXPOSE 8080
CMD ["apollos-ui-navigator"]
