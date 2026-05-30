# Daily Paper

## Rules

- 中文对话，conventional commit 格式（feat:/fix:/chore:/等），不自动提交
- 2-crate workspace：`daily-paper` (bin) + `daily-paper-core` (lib)
- thiserror in core, anyhow at bin boundary
- chrono, reqwest (rustls-tls-native-roots), lettre (sync)
- 测试用 wiremock mock HTTP，fixtures in `tests/fixtures/`
- **不要修改 `.daily-paper/` 目录内容，除非用户明确允许**

## Environment

- fish shell + direnv
- 自签证书 API 代理：`newapi.nuc.home.arpa`
- 项目配置：`.daily-paper/config/config.toml`
