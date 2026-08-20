# Zen Proxy RS

高性能 Rust 实现的 OpenAI 兼容 API 反向代理，面向 Claude Code、Hermes、OpenClaw 等客户端场景。支持代理节点池调度、协议修复、动态模型发现与可观测性管理接口。

本仓库为可独立部署的开源发行版，已移除内部运维文档、节点配置与密钥等敏感信息。

**官方 contributor** 见 [CONTRIBUTORS](./CONTRIBUTORS)；未经维护者许可不得将 Cursor 等 AI agent 列为贡献者（见 [CONTRIBUTING.md](./CONTRIBUTING.md)）。

## 功能概览

- OpenAI 兼容 `/v1/*` 代理转发
- 可选 `free_model_kernel` 模式（内嵌 `free-model-client-rs` 内核）
- SOCKS5/HTTP 代理节点池与故障隔离
- 动态模型发现与探针（可选）
- Prometheus `/metrics`、管理后台 `/admin/*`
- Redis 全局预算与会话亲和（可选）
- **流式截断防护**（v0.3.0）：DSML 标记 withhold/repair、短句假 `end_turn` 的 unfinished tool-intent 检测与重试

## 快速开始

### 前置要求

- Rust 1.75+（推荐 stable）
- 可选：Redis（全局预算 / 会话 pin）

### 本地运行

```bash
cp .env.example .env
# 编辑 .env，至少设置 UPSTREAM_API_KEY

./setup.sh
set -a && source .env && set +a   # Linux/macOS
./zen-proxy-rs/target/release/zen-proxy-rs
```

默认监听 `127.0.0.1:4000`。

验证：

```bash
curl -fsS http://127.0.0.1:4000/health | jq .
```

### Docker（单机）

```bash
cp .env.example .env
# 编辑 .env
docker compose up -d --build
```

服务地址：`http://127.0.0.1:4000`

## 部署流程

以下流程适用于**单台 Linux 服务器**上的生产部署。多实例滚动升级见下一节。

### 1. 构建发布二进制

在构建机或目标机上：

```bash
git clone https://github.com/croppedtravelleralex/Zenproxyrs.git
cd Zenproxyrs
cp .env.example .env
# 编辑 .env（UPSTREAM_API_KEY、PROXY_API_KEY 等）

./setup.sh
# 产物：zen-proxy-rs/target/release/zen-proxy-rs
```

记录构建指纹（部署后与健康检查对比）：

```bash
sha256sum zen-proxy-rs/target/release/zen-proxy-rs
./zen-proxy-rs/target/release/zen-proxy-rs --version 2>/dev/null || true
```

### 2. 安装目录与配置

推荐布局：

```text
/opt/zen-proxy-rs/zen-proxy-rs          # 当前运行二进制（可按版本命名）
/etc/zen-proxy-rs/.env                  # 环境变量（勿入库）
/etc/zen-proxy/nodes.json               # 代理节点池（若使用）
/etc/systemd/system/zen-proxy-rs.service
```

```bash
sudo mkdir -p /opt/zen-proxy-rs /etc/zen-proxy-rs /etc/zen-proxy
sudo install -m 0755 zen-proxy-rs/target/release/zen-proxy-rs /opt/zen-proxy-rs/zen-proxy-rs
sudo cp .env.example /etc/zen-proxy-rs/.env
sudo cp nodes.json.example /etc/zen-proxy/nodes.json
# 编辑 /etc/zen-proxy-rs/.env 与 nodes.json
```

在 `.env` 或 systemd 中设置（生产必做）：

- `PROXY_API_KEY` — 客户端鉴权
- `ADMIN_API_KEY` — 管理接口鉴权
- `BIND_ADDRESS` / `PORT` — 监听地址
- `NODES_FILE=/etc/zen-proxy/nodes.json`（若走节点池）

### 3. systemd 服务示例

`/etc/systemd/system/zen-proxy-rs.service`：

```ini
[Unit]
Description=Zen Proxy RS
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=zen-proxy
Group=zen-proxy
EnvironmentFile=/etc/zen-proxy-rs/.env
ExecStart=/opt/zen-proxy-rs/zen-proxy-rs
Restart=on-failure
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now zen-proxy-rs
sudo systemctl status zen-proxy-rs
```

### 4. 部署后验收

```bash
# 健康检查应返回 status=ok，并含 git_hash / build_time
curl -fsS http://127.0.0.1:4000/health | jq .

# 指标端点（若未限制网络，仅内网访问）
curl -fsS http://127.0.0.1:4000/metrics | head

# 带鉴权的模型列表（若设置了 PROXY_API_KEY）
curl -fsS -H "Authorization: Bearer $PROXY_API_KEY" \
  http://127.0.0.1:4000/v1/models | jq '.data[0].id'
```

建议观察 10–30 分钟：`5xx`、空输出、流式中断率、Prometheus 指标。

### 5. 滚动升级（多实例 / Canary）

若运行多个实例（不同端口或不同 systemd unit，例如 `zen-proxy-rs@1`、`@2`、`@3`）：

1. **选一台 canary**（流量较小或专用端口）部署新版本二进制。
2. 备份当前二进制：`cp current-binary current-binary.bak-$(date +%Y%m%d)`.
3. 通过 systemd drop-in 或替换 `/opt/zen-proxy-rs/` 下版本化文件名，更新 `ExecStart`。
4. `systemctl daemon-reload && systemctl restart <canary-unit>`。
5. 验收 canary 的 `/health`（`git_hash` 与 `sha256` 与预期一致）、抽样真实客户端请求。
6. **其余实例依次滚动**，每次重启一台并短窗口观察。
7. 全部完成后保留一份旧二进制以便回滚。

通过 **GitHub Release 二进制分发**（内网服务器拉取，避免 scp 大文件）的通用模式：

1. 在 CI 或构建机 `cargo build --release`，为产物打版本名（如 `zen-proxy-rs.v0.3.0`）。
2. 创建**临时或正式** GitHub Release，上传二进制；记录 `sha256sum`。
3. 目标机用 `curl` 下载 release asset，校验 SHA256 后 `install` 到 `/opt/zen-proxy-rs/`。
4. 按上文 canary → 全量步骤重启服务。

### 6. 回滚

```bash
sudo systemctl stop zen-proxy-rs
sudo install -m 0755 /opt/zen-proxy-rs/zen-proxy-rs.bak-YYYYMMDD \
  /opt/zen-proxy-rs/zen-proxy-rs
sudo systemctl start zen-proxy-rs
curl -fsS http://127.0.0.1:4000/health | jq .
```

Docker 回滚：`docker compose down` 后切回上一镜像 tag 或 `docker compose up -d --build` 使用上一 git tag。

### 7. Docker 生产注意点

- 将 `.env` 放在 compose 外并通过 `env_file` 引用，勿把密钥写进镜像层。
- 仅暴露必要端口；`ADMIN_API_KEY` 与 `/admin/*` 不要对公网开放。
- 与 systemd 部署相同：设置 `PROXY_API_KEY`、健康检查与日志采集。

## 目录结构

```text
.
├── free-model-client-rs/   # 内嵌上游协议内核库
├── zen-proxy-rs/           # 主代理服务
├── .env.example            # 环境变量模板
├── nodes.json.example      # 代理节点池示例
├── Dockerfile
├── docker-compose.yml
├── setup.sh                # 一键构建脚本
├── CONTRIBUTORS            # 官方 contributor 名单
└── CONTRIBUTING.md
```

## 核心环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `PORT` | `4000` | 监听端口 |
| `BIND_ADDRESS` | `127.0.0.1` | 绑定地址 |
| `UPSTREAM_BASE` | `https://opencode.ai/zen` | 上游 API 根地址 |
| `UPSTREAM_API_KEY` | `public` | 上游 API Key |
| `PROXY_API_KEY` | _(空)_ | 客户端鉴权 Key，留空则不校验 |
| `ADMIN_API_KEY` | _(空)_ | 管理接口鉴权 Key |
| `ZEN_PROVIDER_MODE` | `legacy` | `legacy` 或 `free_model_kernel` |
| `NODES_FILE` | `/etc/zen-proxy/nodes.json` | 代理节点列表文件 |
| `PREFERRED_PROXY_URLS` | _(空)_ | 优先代理 URL，逗号分隔 |
| `GLOBAL_BUDGET_REDIS_URL` | _(空)_ | Redis 地址（全局预算） |

完整列表见 [.env.example](./.env.example)。

## 代理节点配置

将 `nodes.json.example` 复制为节点文件，例如：

```bash
sudo mkdir -p /etc/zen-proxy
sudo cp nodes.json.example /etc/zen-proxy/nodes.json
```

支持 JSON 数组或 `host:port:user:pass` 行格式。

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/metrics` | Prometheus 指标 |
| GET | `/v1/models` | 模型列表 |
| ANY | `/v1/*` | OpenAI 兼容代理 |
| * | `/admin/*` | 管理接口（需 `ADMIN_API_KEY`） |

## 构建与测试

```bash
./setup.sh
cd zen-proxy-rs
cargo test
cd ../free-model-client-rs
cargo test
```

## 版本与 Release

- 语义化 tag：`v0.3.0` 及以后见 [GitHub Releases](https://github.com/croppedtravelleralex/Zenproxyrs/releases)。
- `v0.3.0`：DSML 截断泄漏防护、unfinished tool-intent 假 `end_turn` 重试、流式 guard 相关修复。
- 生产部署建议固定 **git tag + sha256**，通过 `/health` 的 `git_hash` 核对运行版本。

## 许可证

MIT License — 见 [LICENSE](./LICENSE)。Copyright (c) 2026 croppedtravelleralex。

## 安全说明

- 切勿将 `.env`、`nodes.json` 或真实 API Key 提交到版本库
- 生产环境务必设置 `PROXY_API_KEY` 与 `ADMIN_API_KEY`
- 本发行版已剔除内部部署记录、运维 handoff 文档及测试脚本中的节点引用
