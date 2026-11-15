FROM ubuntu:24.04 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cmake \
    curl \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH="/usr/local/cargo/bin:${PATH}"

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable

WORKDIR /build
COPY Cargo.* ./
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
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN arch="$(dpkg --print-architecture)" && \
    case "$arch" in \
        amd64) node_arch="x64" ;; \
        arm64) node_arch="arm64" ;; \
        *) echo "Unsupported architecture: $arch" && exit 1 ;; \
    esac && \
    curl -fsSL "https://nodejs.org/dist/v22.14.0/node-v22.14.0-linux-${node_arch}.tar.xz" \
    | tar -xJ --strip-components=1 -C /usr/local

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
