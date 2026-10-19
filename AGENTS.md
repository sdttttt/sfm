# 仓库指南（Repository Guidelines）

## 项目结构与模块组织

Sfm 是一个用 Rust 编写的小型 macOS 菜单栏 CPU / 内存 / 网速监控应用，基于 AppKit 与 `objc2`。代码全部位于 `src/`：

- `main.rs` —— 应用生命周期、`NSStatusItem`、dispatch 定时器与 `main()` 入口。定时器依赖手写的 `dispatch_source_create` FFI（`dispatch2` 未包装它）；`App` 由 `Box::leak` 故意泄漏，其裸指针经 ivar 交给 `MenuHandler`，靠「永不释放 + 只在主线程」保证安全。改动定时器或 App 生命周期前先读这里。
- `metrics/` —— 采样层，统一走 `sysinfo`。`mod.rs` 定义 `Snapshot` 与同时持有 `System`、`Networks` 的 `Sampler`（网速速率 = 两次刷新的字节差 ÷ 真实间隔，`Instant` 记在 `Sampler` 里）；`cpu.rs`、`mem.rs`、`net.rs` 分别封装 CPU、内存/交换、网络接口的读取。平台相关的细节都收敛在本模块内，上层只见 `Snapshot`。
- `format.rs` —— 文本格式化，含 `MemStyle`（内存显示风格）。纯字符串逻辑，可在无 GUI 环境下单测。
- `menu.rs` —— 菜单项及 target/action 接线。「端口范围」的接口名是运行时数据，各项共用 `toggleNetIface:` 一个 selector，靠菜单项 tag（= `MenuParts.net_ifaces` 下标）反查接口。
- `prefs.rs` —— 通过 `NSUserDefaults` 持久化刷新间隔、内存显示风格、网速开关与端口范围勾选集合。

打包相关文件：

- `Cargo.toml` 的 `[package.metadata.bundle]` —— cargo-bundle 的清单配置（名称、identifier、最低系统版本）。
- `packaging/macos/Info.plist-ext.plist` —— 追加进自动生成 plist 的键，目前是 `LSUIElement`（隐藏 Dock 图标，菜单栏应用必需）。
- `packaging/macos/com.kms.sfm.launchagent.plist` —— 登录项模板。

产物在 `target/release/bundle/osx/Sfm.app`，已由 `/target` 规则忽略。

## 构建、测试与开发命令

打包由 **cargo-bundle** 完成，`cargo app` 是 `.cargo/config.toml` 里的别名，等价于 `cargo bundle --release --format osx`。限定 `--format osx` 是为了跳过默认的 `.dmg` 步骤（菜单栏工具不需要磁盘镜像）。

`cargo-bundle` **不是 dev-dependency**，`cargo app` 前必须已全局安装，否则报 `no such command: bundle`：

```bash
cargo install cargo-bundle
```

- `cargo app` —— release 构建并组装 `Sfm.app`。
- `cargo test` —— 运行全部单元测试（默认 22 项，无需 GUI）。
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

格式化与 lint 规则固化在 `rustfmt.toml`（只用 stable 选项：4 空格缩进、宽度 100）与 `Cargo.toml` 的 `[lints]`（`undocumented_unsafe_blocks` 会把缺失的 `// SAFETY:` 报出来）。仓库没有 CI，因此提交前手动跑 `cargo fmt` 与 `cargo clippy --all-targets --all-features -- -D warnings`。模块用 `snake_case`，类型用 `PascalCase`，函数与变量用 `snake_case`，常量用 `SCREAMING_SNAKE_CASE`。保留文档注释（模块用 `//!`，公开项用 `///`）——现有文件注释充分，请解释**为什么**而非**做了什么**。

## 测试规范

测试以内联 `#[cfg(test)] mod tests` 的形式写在被测模块中（`format.rs`、`metrics/mod.rs`）。用行为命名测试函数，例如 `title_width_is_stable_across_percentages`、`refresh_reports_plausible_memory`。跑单个测试用 `cargo test <test_name>`（如 `cargo test refresh_reports_plausible_memory`）。任何涉及采样或格式化的改动都必须保持 `cargo test` 通过，并为新增边界补测试；若改动了菜单接线，还需同步扩展 `--selftest`。

## 提交与 Pull Request 规范

提交信息简短、祈使句式，并以模块名开头，例如 `metrics: reset baseline after sleep`；正文用要点列出行为变化与理由。PR 需说明对用户可见的变化，附上 `cargo test` 与 `--selftest` 结果；涉及 UI 改动时附菜单栏截图。

## Agent 专属注意事项

切勿提交 `/target`。除非确有需要，不要新增 `objc2` feature——`Cargo.toml` 中的 feature 列表是刻意精简的。`sysinfo` 必须保持 `default-features = false` 且只开 `system` + `network`（network 供网速采样），否则默认会拖进 disk/component/user。改动 `[package.metadata.bundle]` 后需实跑 `--selftest` 确认状态栏项仍然出现（`status item visible: true`），因为 `LSUIElement` 之类影响 GUI 行为的键无法靠单元测试覆盖。
