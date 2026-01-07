# Multi-stage Dockerfile for GhostWire Server on Fly.io
# Stage 1: Build the Rust application
FROM rust:1.75 AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml ./
COPY server/Cargo.toml ./server/
COPY client/Cargo.toml ./client/

# Create dummy source files to cache dependencies
RUN mkdir -p server/src client/src && \
  echo "fn main() {}" > server/src/main.rs && \
  echo "fn main() {}" > server/src/local.rs && \
  echo "fn main() {}" > client/src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release -p ghostwire-server

# Remove dummy files
RUN rm -rf server/src client/src

# Copy actual source code
COPY server/src ./server/src

# Build the actual application
# Touch main.rs to force rebuild of the application code
RUN touch server/src/main.rs && \
  cargo build --release -p ghostwire-server

# Stage 2: Create minimal runtime image
FROM debian:bookworm-slim

# Install CA certificates for HTTPS (if needed for redirects)
RUN apt-get update && \
  apt-get install -y ca-certificates && \
  rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/ghostwire-server /usr/local/bin/ghostwire-server

# Set environment variable for port (Fly.io will inject this)
ENV PORT=8080
ENV RUST_LOG=info

# Expose the port
EXPOSE 8080

# Run the server
CMD ["ghostwire-server"]
