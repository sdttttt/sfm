# Sfm —— 构建并运行这个菜单栏应用。
#
# 只有当进程从真实 bundle 中运行时，`NSStatusItem` 才能在菜单栏占到位，所以光有
# `cargo run` 并不够：`build` 会组装 Sfm.app 并做 ad-hoc 签名。
#
# 注意：请双击 `Sfm.app` 本身，不要去双击 `Sfm.app/Contents/MacOS` 里的二进制
# —— Finder 会在 Terminal 中运行裸可执行文件，于是那个 Terminal 窗口看起来
# 就好像 app「打开了一个窗口」。

app := "Sfm"
bundle := app + ".app"
binary := "target/release/sfm"
bundle_path := justfile_directory() / bundle
label := "com.kms.sfm"
agent := home_directory() / "Library/LaunchAgents" / label + ".plist"
domain := "gui/" + `id -u`

# 由 release 构建组装出 Sfm.app。
default: build

build:
    cargo build --release
    rm -rf "{{bundle}}"
    mkdir -p "{{bundle}}/Contents/MacOS" "{{bundle}}/Contents/Resources"
    cp "{{binary}}" "{{bundle}}/Contents/MacOS/sfm"
    cp Info.plist "{{bundle}}/Contents/Info.plist"
    # 本地使用 ad-hoc 签名就够了，不需要 Developer ID。
    codesign --force --sign - --timestamp=none "{{bundle}}"
    @echo "built {{bundle}}"

# 从终端而不是用 `open` 运行，这样这里能看到 panic。
run: build
    cd "{{bundle}}/Contents/MacOS" && ./sfm

# 运行应用，并把每次刷新的一行输出打到 stderr。
debug: build
    cd "{{bundle}}/Contents/MacOS" && SFM_DEBUG=1 ./sfm

# 静默启动：不弹 Terminal 窗口、不显示 Dock 图标，只驻留菜单栏。
start: build
    open "{{bundle_path}}"

# 停止由 `start` 或登录项启动的应用。
stop:
    launchctl bootout {{domain}}/{{label}} 2>/dev/null || pkill -x sfm || true

# 运行单元测试。
test:
    cargo test

# 在无人点击的情况下校验每个菜单 action，并打印显示出来的值。
selftest: build
    cd "{{bundle}}/Contents/MacOS" && ./sfm --selftest

# 安装登录项，使 Sfm 从下次登录开始就在后台运行。
install: build
    mkdir -p "`dirname {{agent}}`"
    printf '%s\n' \
      '<?xml version="1.0" encoding="UTF-8"?>' \
      '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
      '<plist version="1.0">' \
      '<dict>' \
      '    <key>Label</key>' \
      '    <string>{{label}}</string>' \
      '    <key>ProgramArguments</key>' \
      '    <array>' \
      '        <string>{{bundle_path}}/Contents/MacOS/sfm</string>' \
      '    </array>' \
      '    <key>RunAtLoad</key>' \
      '    <true/>' \
      '    <key>ProcessType</key>' \
      '    <string>Interactive</string>' \
      '    <key>LimitLoadToSessionType</key>' \
      '    <string>Aqua</string>' \
      '</dict>' \
      '</plist>' > {{agent}}
    plutil -lint {{agent}}
    launchctl bootout {{domain}}/{{label}} 2>/dev/null || true
    launchctl bootstrap {{domain}} {{agent}}
    @echo "installed {{agent}} — runs in the background from the next login on"

# 移除登录项并停止应用。
uninstall:
    launchctl bootout {{domain}}/{{label}} 2>/dev/null || true
    rm -f {{agent}}
    @echo "removed {{agent}}"

# 清除构建产物。
clean:
    cargo clean
    rm -rf "{{bundle}}"
