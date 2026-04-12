# Stage 2: Final Runtime Image
# Use a minimal base image for the final container to reduce size
FROM debian:bullseye-slim

# Set environment variables
ENV RUST_VERSION=1.0
ENV PATH="/usr/local/cargo/bin:${PATH}"

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/docker-infolit /usr/local/bin/server

# Expose the port the web server will listen on
EXPOSE 3000

# Command to run the server when the container starts
CMD ["/usr/local/bin/server"]