# --- Stage 1: Build ---
    FROM rust:1.75-slim-bookworm AS builder

    WORKDIR /usr/src/response_checker
    
    COPY Cargo.toml Cargo.lock* ./
    
    # Download and cache dependencies (this step speeds up rebuilds)
    RUN cargo fetch --locked
    
    COPY src ./src/
    
    RUN cargo build --release

# --- Stage 2: Run ---
    FROM debian:bookworm-slim AS runtime
    
    # Install any necessary runtime dependencies (if any, for this script there are very few, glibc is usually enough which is in slim)
    # RUN apt-get update && apt-get install -y --no-install-recommends <runtime-dependencies> && rm -rf /var/lib/apt/lists/*
    
    WORKDIR /app
    
    COPY --from=builder /usr/src/response_checker/target/release/response_checker ./
    
    # Command to run the application.
    # Now only expects CONFIG_FILE path as argument (default --file mode)
    # Or can be run with --directory <dir_path>
    CMD ["./response_checker", "config.json"]