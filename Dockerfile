# joplin-lite —— 多阶段构建：前端(node) + 后端(rust) → 精简运行镜像

# ---- 1) 构建前端 (web/dist) ----
FROM node:20-slim AS webbuild
WORKDIR /web
RUN npm install -g pnpm@10
# 先装依赖（利用层缓存）
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# ---- 2) 构建后端 (release 二进制) ----
FROM rust:1-bookworm AS rustbuild
# core 是 server 的路径依赖（纯逻辑 crate），先拷入
COPY core /build/core
WORKDIR /build/server
# 先用清单 + 占位 main 构建依赖层（rusqlite/axum 等编译较久，缓存复用）
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release \
 && rm -rf src
# 再构建真正的二进制
COPY server/src ./src
RUN touch src/main.rs && cargo build --release

# ---- 3) 运行镜像 ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rustbuild /build/server/target/release/joplin-lite /app/joplin-lite
COPY --from=webbuild /web/dist /app/web

ENV JOPLIN_LITE_WEB_DIR=/app/web \
    JOPLIN_LITE_HOST=0.0.0.0 \
    JOPLIN_LITE_PORT=27583 \
    JOPLIN_LITE_CONFIG_DIR=/config

# 配置库（数据源设置）持久化到该卷
VOLUME ["/config"]
EXPOSE 27583
CMD ["/app/joplin-lite"]
