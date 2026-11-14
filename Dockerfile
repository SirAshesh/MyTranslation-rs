FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN if [ -d src/pipe/free_mode ]; then \
        cargo build --release --bin server-rs; \
    else \
        cargo build --release --no-default-features --bin server-rs; \
    fi && \
    strip /build/target/release/server-rs

FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libgomp1 \
    libstdc++6 \
    libssl3t64 \
    pciutils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/server-rs /app/server-rs
COPY fonts /app/fonts

RUN mkdir -p /app/models /app/data && \
    curl -fsSL -o /app/models/en_PP-OCRv3_det_mobile.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.0/onnx/PP-OCRv4/det/en_PP-OCRv3_det_mobile.onnx" && \
    curl -fsSL -o /app/models/en_PP-OCRv4_rec_mobile.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.0/onnx/PP-OCRv4/rec/en_PP-OCRv4_rec_mobile.onnx" && \
    curl -fsSL -o /app/models/ch_ppocr_mobile_v2.0_cls_mobile.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.0/onnx/PP-OCRv4/cls/ch_ppocr_mobile_v2.0_cls_mobile.onnx" && \
    curl -fsSL -o /app/models/en_dict.txt "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/master/paddle/PP-OCRv4/rec/en_PP-OCRv4_rec_mobile/en_dict.txt" && \
    chmod +x /app/server-rs

ENV PORT=5000 \
    DATA_DIR=/app/data \
    RUST_LOG=info

EXPOSE 5000
VOLUME ["/app/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT}/login" >/dev/null || exit 1

CMD ["/app/server-rs"]
