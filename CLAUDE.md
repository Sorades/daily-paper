# Daily Paper

## Rules

- 中文对话，conventional commit 格式（feat:/fix:/chore:/等），不自动提交
- 2-crate workspace：`daily-paper` (bin) + `daily-paper-core` (lib)
- thiserror in core, anyhow at bin boundary
- chrono, reqwest (rustls-tls-native-roots), lettre (sync)
- 测试用 wiremock mock HTTP，fixtures in `tests/fixtures/`

## Environment

- fish shell + direnv
- 自签证书 API 代理：`newapi.nuc.home.arpa`
- 测试配置：`target/temp/config.toml`
