FROM rust:1.59-alpine AS runtime
RUN rustup target add x86_64-unknown-linux-gnu
