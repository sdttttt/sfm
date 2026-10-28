# Sfm

macOS 菜单栏 CPU / 内存 / 网速监控，纯 Rust 直接调用 AppKit，无运行时依赖。

```
CPU= 13% MEM= 67% ↓ 340K ↑1.2M
```

## 功能

- 菜单栏显示 CPU、内存与上下行网速；内存可切换百分比 / 占用大小两种风格
- 刷新间隔 1 / 2 / 5 秒
- 网速统计范围按网卡接口勾选，默认只算物理网卡（`en*`）
- 所有设置自动保存，重启后保留

## 构建

```bash
cargo install cargo-bundle   # 仅首次
cargo app                    # 产出 target/release/bundle/osx/Sfm.app
```

## 运行

```bash
open target/release/bundle/osx/Sfm.app   # 启动
pkill -x sfm                             # 停止
```

必须从 `.app` 运行：`NSStatusItem` 只在真实 bundle 中才能占到菜单栏位置，
`cargo run` 或裸二进制不会显示任何东西（双击也请点 `Sfm.app` 本身）。
从终端调试时进入 `Sfm.app/Contents/MacOS` 运行 `./sfm`，可加 `SFM_DEBUG=1`
（每帧打一行）或 `--selftest`（无人点击校验菜单接线）。

## 测试

```bash
cargo test
```

涉及菜单接线的改动需用 `--selftest` 实跑，确认首行 `status item visible: true`。

## 开机启动

详见 [packaging/macos/com.kms.sfm.launchagent.plist](packaging/macos/com.kms.sfm.launchagent.plist)。

## 环境

macOS 11+，Rust 2021 edition。贡献见 [AGENTS.md](AGENTS.md)。
