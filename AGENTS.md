# 仓库指南（Repository Guidelines）

## 项目结构与模块组织

Sfm 是一个用 Rust 编写的小型 macOS 菜单栏 CPU / 内存监控应用，基于 AppKit 与 `objc2`。代码全部位于 `src/`：

- `main.rs` —— 应用生命周期、`NSStatusItem`、dispatch 定时器与 `main()` 入口。
- `metrics/` —— 采样层。`mod.rs` 定义共享的 `Snapshot`；`cpu.rs`、`mem.rs` 存放平台读取逻辑。**所有对内核的裸 C FFI 调用必须收敛在本模块内**，且每处都要有 `// SAFETY:` 注释。
- `format.rs` —— 文本格式化，含 `MemStyle`（内存显示风格）。纯字符串逻辑，可在无 GUI 环境下单测。
- `menu.rs` —— 菜单项及 target/action 接线。
- `prefs.rs` —— 通过 `NSUserDefaults` 持久化刷新间隔与内存显示风格。

打包相关文件：

- `Cargo.toml` 的 `[package.metadata.bundle]` —— cargo-bundle 的清单配置（名称、identifier、最低系统版本）。
- `packaging/macos/Info.plist-ext.plist` —— 追加进自动生成 plist 的键，目前是 `LSUIElement`（隐藏 Dock 图标，菜单栏应用必需）。
- `packaging/macos/com.kms.sfm.launchagent.plist` —— 登录项模板。

产物在 `target/release/bundle/osx/Sfm.app`，已由 `/target` 规则忽略。

## 构建、测试与开发命令

打包由 **cargo-bundle** 完成，`cargo app` 是 `.cargo/config.toml` 里的别名，等价于 `cargo bundle --release --format osx`。限定 `--format osx` 是为了跳过默认的 `.dmg` 步骤（菜单栏工具不需要磁盘镜像）。

- `cargo app` —— release 构建并组装 `Sfm.app`（无需 ad-hoc 签名，链接器已自动附加）。
- `cargo test` —— 运行单元测试。
- `cargo clean` —— 清除构建产物（含 bundle）。

直接 `cargo run` **无法工作**：`NSStatusItem` 只有在进程从真实 bundle 中运行时才会占用菜单栏槽位。因此调试与运行都要走 bundle 内的二进制：

```bash
# 从仓库根目录执行
cargo app                                             # 先产出 bundle
open target/release/bundle/osx/Sfm.app                # 正常 GUI 启动，无终端窗口

cd target/release/bundle/osx/Sfm.app/Contents/MacOS   # 或从终端运行
./sfm                              # panic 可见
SFM_DEBUG=1 ./sfm                  # 每次刷新向 stderr 输出一行
./sfm --selftest                   # 无人点击校验菜单接线，打印显示值
pkill -x sfm                    # 停止（全局命令，无需 cd）
```

注意：**双击 `Sfm.app` 本身**，不要双击 `Contents/MacOS` 里的二进制 —— Finder 会在 Terminal 中运行裸可执行文件，那个窗口看起来就像 app「打开了一个窗口」。

登录项（开机后台运行）见 `packaging/macos/com.kms.sfm.launchagent.plist` 的注释说明。

## 编码风格与命名约定

采用标准 `rustfmt`（4 空格缩进，无自定义配置），并保持 clippy 无告警。模块用 `snake_case`，类型用 `PascalCase`，函数与变量用 `snake_case`，常量用 `SCREAMING_SNAKE_CASE`。保留文档注释（模块用 `//!`，公开项用 `///`）——现有文件注释充分，请解释**为什么**而非**做了什么**。

## 测试规范

测试以内联 `#[cfg(test)] mod tests` 的形式写在被测模块中（`format.rs`、`metrics/cpu.rs`）。用行为命名测试函数，例如 `first_frame_is_dropped`、`failed_read_keeps_previous_frame`。任何涉及采样或格式化的改动都必须保持 `cargo test` 通过，并为新增边界补测试；若改动了菜单接线，还需同步扩展 `--selftest`。

## 提交与 Pull Request 规范

提交信息简短、祈使句式，并以模块名开头，例如 `metrics: reset baseline after sleep`；正文用要点列出行为变化与理由。PR 需说明对用户可见的变化，附上 `cargo test` 与 `--selftest` 结果；涉及 UI 改动时附菜单栏截图。

## Agent 专属注意事项

切勿提交 `/target`。除非确有需要，不要新增 `objc2` feature——`Cargo.toml` 中的 feature 列表是刻意精简的。改动 `[package.metadata.bundle]` 后需实跑 `--selftest` 确认状态栏项仍然出现（`status item visible: true`），因为 `LSUIElement` 之类影响 GUI 行为的键无法靠单元测试覆盖。
