# Daily Paper

## Rules

- 中文对话，conventional commit 格式（feat:/fix:/chore:/等），不自动提交
- 单 Crate 项目架构：`daily-paper` (`src/lib.rs` + `src/main.rs`)，模块内聚清晰
- 全局强类型 thiserror 错误处理，bin 边界/CLI 使用 anyhow
- chrono, reqwest (rustls-tls-native-roots), lettre (sync)
- 测试用 wiremock mock HTTP，fixtures in `tests/fixtures/`
- 不要修改配置目录内容，除非用户明确允许
- 修改完成后，自行使用 `clippy`, `fmt` 检查
