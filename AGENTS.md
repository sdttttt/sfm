# 仓库指南（Repository Guidelines）

## 项目结构与模块组织

Sfm 是一个用 Rust 编写的小型 macOS 菜单栏 CPU / 内存监控应用，基于 AppKit 与 `objc2`。代码全部位于 `src/`：

- `main.rs` —— 应用生命周期、`NSStatusItem`、dispatch 定时器与 `main()` 入口。
- `metrics/` —— 采样层。`mod.rs` 定义共享的 `Snapshot`；`cpu.rs`、`mem.rs` 存放平台读取逻辑。**所有 `unsafe` FFI 必须收敛在该模块内**，且每处都要有 `// SAFETY:` 注释。
- `format.rs` —— 把 `Snapshot` 渲染为菜单栏标题文本。
- `menu.rs` —— 菜单项及 target/action 接线。
- `prefs.rs` —— 通过 `NSUserDefaults` 持久化刷新间隔。

`Info.plist` 与 `justfile` 共同组装 `Sfm.app`；`target/` 和 `Sfm.app/` 均为构建产物，已在 `.gitignore` 中忽略。

## 构建、测试与开发命令

统一使用 [`just`](https://github.com/casey/just)。直接 `cargo run` 无法工作：`NSStatusItem` 只有在真实 bundle 中运行才会占用菜单栏槽位。

- `just build` —— release 构建，并组装、ad-hoc 签名 `Sfm.app`。
- `just run` —— 构建后从终端启动，便于查看 panic。
- `just debug` —— 同上，另加 `SFM_DEBUG=1`（每次刷新向 stderr 输出一行）。
- `just start` / `just stop` —— 静默启动（仅菜单栏、无 Dock 图标）或终止进程。
- `just test` —— 运行单元测试（即 `cargo test`）。
- `just selftest` —— 无人点击的情况下校验全部菜单 action，并打印显示值。
- `just install` / `just uninstall` —— 管理 `~/Library/LaunchAgents` 登录项。
- `just clean` —— 清理构建产物。

## 编码风格与命名约定

采用标准 `rustfmt`（4 空格缩进，无自定义配置），并保持 clippy 无告警。模块用 `snake_case`，类型用 `PascalCase`，函数与变量用 `snake_case`，常量用 `SCREAMING_SNAKE_CASE`。保留文档注释（模块用 `//!`，公开项用 `///`）——现有文件注释充分，请解释**为什么**而非**做了什么**。

## 测试规范

测试以内联 `#[cfg(test)] mod tests` 的形式写在被测模块中（`format.rs`、`metrics/cpu.rs`）。用行为命名测试函数，例如 `first_frame_is_dropped`、`failed_read_keeps_previous_frame`。任何涉及采样或格式化的改动都必须保持 `just test` 通过，并为新增边界补测试；若改动了菜单接线，还需同步扩展 `just selftest`。

## 提交与 Pull Request 规范

提交信息简短、祈使句式，并以模块名开头，例如 `metrics: reset baseline after sleep`；正文用要点列出行为变化与理由。PR 需说明对用户可见的变化，附上 `just test` / `just selftest` 结果；涉及 UI 改动时附菜单栏截图。

## Agent 专属注意事项

切勿提交 `/target` 或 `/Sfm.app`。除非确有需要，不要新增 `objc2` feature——`Cargo.toml` 中的 feature 列表是刻意精简的。
