# --- Stage 1: Build ---
    FROM rust:1.85.0-bookworm AS builder

    WORKDIR /usr/src/release-sanity-checker
    
    COPY Cargo.toml Cargo.lock* ./
    COPY src ./src/
    
    # Download and cache dependencies (this step speeds up rebuilds)
    RUN cargo fetch --locked
    RUN cargo build --release

# --- Stage 2: Run ---
    FROM debian:bookworm-slim AS runtime
    
    # Install any necessary runtime dependencies (if any, for this script there are very few, glibc is usually enough which is in slim)
    # RUN apt-get update && apt-get install -y --no-install-recommends <runtime-dependencies> && rm -rf /var/lib/apt/lists/*
    RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
    
    WORKDIR /app
    
    COPY --from=builder /usr/src/release-sanity-checker/target/release/release-sanity-checker ./
    
    # Command to run the application.
    # Now only expects CONFIG_FILE path as argument (default --file mode)
    # Or can be run with --directory <dir_path>
    #CMD ["./release-sanity-checker", "config.json"]
    # Set the binary as the entrypoint
    ENTRYPOINT ["./release-sanity-checker"]