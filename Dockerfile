FROM rust:1.89-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    ca-certificates \
    fonts-liberation \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_EXECUTABLE=/usr/bin/chromium
ENV BROWSER_HEADLESS=true
ENV PORT=8080

COPY --from=builder /app/target/release/apollos-ui-navigator /usr/local/bin/apollos-ui-navigator

EXPOSE 8080
CMD ["apollos-ui-navigator"]
