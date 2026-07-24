FROM rust:slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli --no-confirm --locked
RUN rustup target add wasm32-unknown-unknown
WORKDIR /app
COPY . .
RUN dx build --platform web --release

FROM nginx:alpine
COPY --from=builder /app/target/dx/dioxus-app/release/web/public /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
