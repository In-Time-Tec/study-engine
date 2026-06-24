FROM node:20-alpine AS node-build
WORKDIR /app
COPY study-engine-ui/package*.json ./study-engine-ui/
RUN cd study-engine-ui && npm install
COPY study-engine-ui/ ./study-engine-ui/
RUN cd study-engine-ui && npm run build

FROM rust:1.88-slim AS rust-build
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY study-engine-cli/ ./study-engine-cli/
RUN cargo build --release --manifest-path study-engine-cli/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-build /app/study-engine-cli/target/release/study-engine ./
COPY --from=node-build /app/study-engine-ui/dist ./static/
COPY questions/ ./questions/
ENV STUDY_ENGINE_STATIC_DIR=/app/static
ENV STUDY_ENGINE_QUESTIONS_DIR=/app/questions
ENV STUDY_ENGINE_DB_PATH=/data/study-engine.db
EXPOSE 3001
CMD ["./study-engine", "serve"]
