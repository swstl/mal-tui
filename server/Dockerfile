FROM rustlang/rust:nightly as builder

WORKDIR /app
COPY . .

RUN cargo build --release

FROM ubuntu:24.04

RUN apt-get update && apt-get install -y ffmpeg && rm -rf /var/lib/apt/lists/*
RUN useradd -m appuser

WORKDIR /app

COPY --from=builder /app/target/release/server /app/server
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/static /app/static
COPY --from=builder /app/video /app/video
COPY --from=builder /app/apt /app/apt

USER appuser

CMD ["./server"]

