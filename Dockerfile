FROM rust:1.96.0-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Build Tailwind CSS
RUN curl -sL https://github.com/tailwindlabs/tailwindcss/releases/download/v4.3.2/tailwindcss-linux-x64 -o /usr/local/bin/tailwindcss && \
    chmod +x /usr/local/bin/tailwindcss
RUN tailwindcss -i static/input.css -o static/styles.css --minify

FROM gcr.io/distroless/cc-debian13:latest-amd64
COPY --from=builder /app/target/release/axum-template /axum-template
COPY --from=builder /app/static /static
COPY ./migrations /migrations
CMD ["/axum-template"]
