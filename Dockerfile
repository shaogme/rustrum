FROM rust:1.97-slim AS builder

WORKDIR /usr/src/rustrum

# 复制整个工作区
COPY . .

# 编译 rustrum-cli
RUN cargo build --release --bin rustrum-cli

# 运行阶段
FROM debian:bookworm-slim

# 安装一些可能需要的基础运行时库
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 从构建器中复制二进制文件
COPY --from=builder /usr/src/rustrum/target/release/rustrum-cli /usr/local/bin/rustrum-cli

# 设置默认启动命令
ENTRYPOINT ["/usr/local/bin/rustrum-cli"]
