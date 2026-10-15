# Sfm

菜单栏 CPU / 内存监控。纯 Rust，直接调用 AppKit，没有 Electron、没有 WebView、没有运行时依赖。

```
CPU= 13% MEM=10.7G          ← 点击前，菜单栏上就这一行
CPU= 13% MEM= 67%           ← 切到百分比风格后
```

## 特性

- **常驻菜单栏**，无 Dock 图标、无窗口、无 app 菜单
- **标题带标签**：`CPU= 百分比` 与 `MEM=` 数值并排，一眼看清两项指标
- **内存两种显示风格**随时切换：占总内存百分比（默认，`67%`）或具体占用大小（`10.7G`）
- **刷新间隔** 1 / 2 / 5 秒可调
- 设置（间隔、显示风格）持久化，重启后保留
- 约 1300 行代码（含测试），20 项单元测试

## 构建

需要 Rust 工具链，以及 [cargo-bundle](https://crates.io/crates/cargo-bundle)（仅打包时用得到）：

```bash
cargo install cargo-bundle
```

打包：

```bash
cargo app
```

`cargo app` 是 `.cargo/config.toml` 里的别名，等价于 `cargo bundle --release --format osx`。
产物在 `target/release/bundle/osx/Sfm.app`。

> **为什么必须打成 bundle？**
> `NSStatusItem` 只有在进程从真实 `.app` 中运行时才会占用菜单栏槽位。直接
> `cargo run` 或运行 `Contents/MacOS/` 里的裸二进制，菜单栏上不会出现任何东西。

## 运行

```bash
# 正常启动（无终端窗口、无 Dock 图标）
open target/release/bundle/osx/Sfm.app

# 从终端运行 —— panic 和日志可见，调试用
cd target/release/bundle/osx/Sfm.app/Contents/MacOS
./sfm                # 前台运行
SFM_DEBUG=1 ./sfm    # 每次刷新向 stderr 打一行
./sfm --selftest     # 无人点击，校验菜单接线并打印显示值

pkill -x sfm         # 停止
```

> **双击请点 `Sfm.app` 本身**，别点 `Contents/MacOS` 里的二进制 —— Finder 会在
> Terminal 里运行裸可执行文件，那个窗口看起来就像 app「打开了一个窗口」。

## 测试

```bash
cargo test              # 20 项单元测试，无需 GUI
```

覆盖 CPU 利用率计算（含休眠唤醒、时间间隔过密、读取失败回退）、字节格式化边界、
百分比钳制与除零保护、内存风格持久化往返。

UI 接线（菜单 action、勾选状态、状态栏标题是否真的送达 AppKit）无法靠单元测试
覆盖，用 `--selftest` 实跑验证：

```bash
cd target/release/bundle/osx/Sfm.app/Contents/MacOS && ./sfm --selftest
```

第一行 `status item visible: true` 是判断「应用拿到了菜单栏槽位」的关键信号。

## 开机启动

先把 bundle 移到稳定位置（`target/` 会被 `cargo clean` 清掉）：

```bash
mkdir -p ~/Applications
cp -R target/release/bundle/osx/Sfm.app ~/Applications/
```

再安装登录项，详见 [`packaging/macos/com.kms.sfm.launchagent.plist`](packaging/macos/com.kms.sfm.launchagent.plist) 的注释：

```bash
mkdir -p ~/Library/LaunchAgents
sed "s#REPLACE_ME#$USER#" packaging/macos/com.kms.sfm.launchagent.plist \
  > ~/Library/LaunchAgents/com.kms.sfm.plist
plutil -lint ~/Library/LaunchAgents/com.kms.sfm.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.kms.sfm.plist
```

卸载：

```bash
launchctl bootout gui/$(id -u)/com.kms.sfm
rm -f ~/Library/LaunchAgents/com.kms.sfm.plist
```

## 工作原理

单线程、单主队列 —— 采样只是微秒级的内核读取，用主队列定时器比「采样线程 + 跨线程
数据搬运」更简单。`NSApplication.run()` 阻塞在事件循环里，定时器到期和菜单点击都
由 AppKit 回调进 Rust。

| 指标 | 来源 |
|---|---|
| CPU | `host_statistics(HOST_CPU_LOAD_INFO)` 的 tick 计数**差值**，`busy = user + system + nice` |
| 已用内存 | `(active + wired + compressed) × page_size` |
| 总内存 | `sysctlbyname("hw.memsize")` |
| Swap | `sysctlbyname("vm.swapusage")` |

几个不显然的处理：

- **CPU 用差值而非累计值**，`nice` 计入忙；差值用 `checked_sub` 防计数器回绕
- **压缩内存必须计入**，漏掉会少报好几个 GB
- **首帧丢弃**（没有上一次采样就无法算差值）
- **间隔 < 0.2s 的采样丢弃**：tick 粒度粗，几个毫秒的窗口会误报 100%
- **间隔 > 3 倍刷新周期视为休眠**：重置基线而非让菜单栏蹦出一个尖峰
- **采样失败保留旧值**，而不是归零 —— 归零看起来像「机器突然空闲了」
- **百分比右对齐 3 字符** + 等宽数字字体：`9%→10%`、`99%→100%` 时后面的数字不抖动

## 环境

- macOS 11 或更高
- Rust 2021 edition

## 贡献

见 [AGENTS.md](AGENTS.md)。
