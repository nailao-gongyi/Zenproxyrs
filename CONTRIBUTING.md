# Contributing

感谢对本项目的关注。

## 开发环境

```bash
git clone https://github.com/croppedtravelleralex/Zenproxyrs.git
cd Zenproxyrs
cp .env.example .env
./setup.sh
cd zen-proxy-rs
cargo test
```

## 提交规范

- 不要提交 `.env`、`nodes.json` 或任何密钥
- 保持 `cargo fmt` / `cargo clippy` 通过
- PR 请附带测试说明（至少 `cargo test` 通过范围）

## 署名与 Contributors 政策

- **禁止**在 commit message、PR 描述、`Co-authored-by` trailer、README、`CONTRIBUTORS` 或任何对外文档中，将 **Cursor**、其他 AI agent 或自动化工具列为作者、贡献者或 contributor。
- 仅当仓库维护者（`croppedtravelleralex`）**明确书面许可**时，方可例外。
- 官方 contributor 名单以仓库根目录 [`CONTRIBUTORS`](./CONTRIBUTORS) 为准，而非 GitHub 自动统计页。

## 报告问题

请使用 GitHub Issues，并避免在 issue 中粘贴真实 API Key 或代理凭据。
