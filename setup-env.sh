#!/usr/bin/env bash
# ============================================================
# 便携式 Tauri 开发环境配置脚本（macOS 版）
# 用法：在终端中执行   source ./setup-env.sh   （bash / zsh 均可）
# 特点：所有路径基于脚本所在目录推导，整个文件夹可任意移动
#       自动读取 app/.node-version，安装并启用对应 Node
#       在 app 目录执行 npm install（npm 源见 app/.npmrc）
#       补齐 react-devtools 所需的 Electron 二进制（缺失时从镜像下载）
#       自动初始化 Rust 工具链（版本取自 app/src-tauri/rust-toolchain.toml，
#       tools/ 下没有 rustup-init 时从官方源下载）
#       自动进入 src-tauri 安装 Tauri CLI
# ============================================================

# 该脚本通过 source 执行，不使用 set -e（否则任一命令失败都会关闭当前终端）；
# 关键步骤失败时显式 return

# ---------- 0. 项目目录 ----------
# 默认取脚本同级的 app 目录；如需改，直接改这一行
# bash 用 BASH_SOURCE，zsh 用 %x 取被 source 的脚本路径
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    SCRIPT_PATH="${BASH_SOURCE[0]}"
elif [ -n "${ZSH_VERSION:-}" ]; then
    eval 'SCRIPT_PATH="${(%):-%x}"'
else
    SCRIPT_PATH="$0"
fi
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
APP_DIR="$SCRIPT_DIR/app"

# ---------- 1. 以脚本所在目录为根 ----------
ROOT="$SCRIPT_DIR"

# ---------- 2. 推导子目录 ----------
TOOLS_DIR="$ROOT/tools"
SDK_DIR="$ROOT/sdk"
FNM_DIR="$SDK_DIR/fnm"
RUSTUP_HOME="$SDK_DIR/rustup"
CARGO_HOME="$SDK_DIR/cargo"
CARGO_BIN="$CARGO_HOME/bin"

# ---------- 3. 确保目录存在 ----------
mkdir -p "$TOOLS_DIR" "$SDK_DIR" "$FNM_DIR" "$RUSTUP_HOME" "$CARGO_HOME"

# ---------- 4. 导出环境变量（仅当前会话） ----------
export FNM_DIR
export RUSTUP_HOME
export CARGO_HOME

# 把 tools 和 cargo bin 加到 PATH 前面（去重）
add_to_path() {
    local dir="$1"
    case ":$PATH:" in
        *":$dir:"*) ;;
        *) export PATH="$dir:$PATH" ;;
    esac
}
add_to_path "$TOOLS_DIR"
add_to_path "$CARGO_BIN"

# ---------- 5. 初始化 fnm ----------
FNM_EXE="$TOOLS_DIR/fnm"
if [ ! -x "$FNM_EXE" ]; then
    # 兼容通过 Homebrew 安装的 fnm
    if command -v fnm >/dev/null 2>&1; then
        FNM_EXE="$(command -v fnm)"
    else
        echo "[fnm] 未找到 $FNM_EXE，请把 fnm 可执行文件放到 tools/ 目录下" >&2
        return 1 2>/dev/null || exit 1
    fi
fi

if [ -n "${ZSH_VERSION:-}" ]; then FNM_SHELL=zsh; else FNM_SHELL=bash; fi
eval "$("$FNM_EXE" env --use-on-cd --shell "$FNM_SHELL")"

# ---------- 6. 从 app/.node-version 读取并安装/启用 Node ----------
NODE_VER_FILE="$APP_DIR/.node-version"
if [ -f "$NODE_VER_FILE" ]; then
    NODE_VER="$(tr -d '[:space:]' < "$NODE_VER_FILE")"
    if [ -z "$NODE_VER" ]; then
        echo "[node] $NODE_VER_FILE 内容为空，跳过"
    else
        echo "[node] 读取版本: $NODE_VER  (来自 $NODE_VER_FILE)"

        echo "[node] 执行 fnm install $NODE_VER ..."
        "$FNM_EXE" install "$NODE_VER" || {
            echo "[node] fnm install 失败" >&2
            return 1 2>/dev/null || exit 1
        }

        echo "[node] 执行 fnm use $NODE_VER ..."
        "$FNM_EXE" use "$NODE_VER" || {
            echo "[node] fnm use 失败" >&2
            return 1 2>/dev/null || exit 1
        }

        echo "[node] 当前 node 版本: $(node --version)"
    fi
else
    echo "[node] 未找到 $NODE_VER_FILE，跳过 Node 安装"
fi

# ---------- 6.5 安装依赖（npm 源由 app/.npmrc 指定） ----------
if [ -d "$APP_DIR" ]; then
    echo "[npm] 进入 $APP_DIR 执行 npm install ..."
    # electron 的 postinstall 会从 GitHub 下载二进制，网络不通时会无限卡在
    # "Downloading Electron binary..."；这里跳过，交给 6.6 从镜像下载
    (cd "$APP_DIR" && ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm install) || {
        echo "[npm] npm install 失败" >&2
        return 1 2>/dev/null || exit 1
    }
    echo "[npm] npm install 完成"
else
    echo "[npm] 未找到 $APP_DIR，跳过 npm install" >&2
fi

# ---------- 6.6 补齐 react-devtools 所需的 Electron 二进制 ----------
# 详见 docs/React DevTools 集成与排障经验.md。electron 的 postinstall 可能未下载成功
# （dist/ 为空、path.txt 缺失），或卡在 "Downloading Electron binary..."，导致 react-devtools 无法启动。
# 不用 `npm install electron --no-save`：它会装最新版 electron（与锁文件不符，
# 且新版 install.js 在当前 Node 下报 ERR_REQUIRE_ESM）。这里按锁定版本从镜像手动下载解压。
if [ -d "$APP_DIR/node_modules/react-devtools" ]; then
    (
        cd "$APP_DIR" || exit 1
        # 按 react-devtools 的解析路径定位 electron（可能是顶层或嵌套的那份）
        ELECTRON_DIR=$(node -p "const p=require('path');p.dirname(require.resolve('electron/package.json',{paths:[p.dirname(require.resolve('react-devtools/package.json'))]}))") || exit 1
        if node -e "require(process.argv[1])" "$ELECTRON_DIR" >/dev/null 2>&1; then
            echo "[electron] 二进制已就绪，跳过"
            exit 0
        fi
        VERSION=$(node -p "require(process.argv[1] + '/package.json').version" "$ELECTRON_DIR")
        PLATFORM=$(node -p "process.platform")
        ARCH=$(node -p "process.arch")
        case "$PLATFORM" in
            darwin) EXE_PATH="Electron.app/Contents/MacOS/Electron" ;;
            win32)  EXE_PATH="electron.exe" ;;
            *)      EXE_PATH="electron" ;;
        esac
        URL="https://cdn.npmmirror.com/binaries/electron/${VERSION}/electron-v${VERSION}-${PLATFORM}-${ARCH}.zip"
        ZIP=$(mktemp "${TMPDIR:-/tmp}/electron.XXXXXX") || exit 1
        echo "[electron] 二进制缺失，下载 $URL ..."
        curl -L --fail --retry 2 --connect-timeout 20 --max-time 600 -o "$ZIP" "$URL" || { rm -f "$ZIP"; exit 1; }
        rm -rf "$ELECTRON_DIR/dist"
        mkdir -p "$ELECTRON_DIR/dist"
        unzip -q "$ZIP" -d "$ELECTRON_DIR/dist" || { rm -f "$ZIP"; exit 1; }
        rm -f "$ZIP"
        # path.txt 不能有末尾换行；version 文件让 install.js 认为已安装
        printf '%s' "$EXE_PATH" > "$ELECTRON_DIR/path.txt"
        printf 'v%s' "$VERSION" > "$ELECTRON_DIR/dist/version"
        echo "[electron] 已安装 Electron $VERSION 到 $ELECTRON_DIR/dist"
    ) || echo "[electron] 补齐 Electron 失败（React DevTools 将不可用）" >&2
fi

# ---------- 7. 初始化 Rust 工具链 ----------
# 工具链完全装在 sdk/ 下（RUSTUP_HOME / CARGO_HOME），不依赖系统 rustup：
# 系统 rustup（如 Homebrew）的 toolchain 和 cargo 代理都不在这里，直接用会报 "no default toolchain"
SRC_TAURI_DIR="$APP_DIR/src-tauri"
RUSTUP_EXE="$CARGO_BIN/rustup"
RUSTUP_INIT="$TOOLS_DIR/rustup-init"

# 从 rust-toolchain.toml 读取锁定版本，作为默认工具链
RUST_CHANNEL=""
if [ -f "$SRC_TAURI_DIR/rust-toolchain.toml" ]; then
    RUST_CHANNEL="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$SRC_TAURI_DIR/rust-toolchain.toml" | head -n 1)"
fi
RUST_CHANNEL="${RUST_CHANNEL:-stable}"

if [ ! -x "$RUSTUP_EXE" ]; then
    # tools/rustup-init 必须是真正的 rustup-init。Homebrew 的 rustup 是包装脚本，
    # 链接成 rustup-init 也不会进入安装模式，这种情况改为下载官方 rustup-init
    if [ -e "$RUSTUP_INIT" ] && ! "$RUSTUP_INIT" --version 2>/dev/null | grep -q '^rustup-init '; then
        echo "[rust] $RUSTUP_INIT 不是 rustup-init，改为下载官方 rustup-init" >&2
        rm -f "$RUSTUP_INIT"
    fi
    if [ ! -x "$RUSTUP_INIT" ]; then
        case "$(uname -m)" in
            arm64|aarch64) RUST_ARCH=aarch64 ;;
            x86_64)        RUST_ARCH=x86_64 ;;
            *)             RUST_ARCH="$(uname -m)" ;;
        esac
        case "$(uname -s)" in
            Darwin) RUST_TRIPLE="$RUST_ARCH-apple-darwin" ;;
            *)      RUST_TRIPLE="$RUST_ARCH-unknown-linux-gnu" ;;
        esac
        # 依次尝试官方源和国内镜像；已设置 RUSTUP_UPDATE_ROOT/RUSTUP_DIST_SERVER 时只用它。
        # 下载成功的源同时用于本会话的工具链下载（导出 RUSTUP_DIST_SERVER）
        if [ -n "${RUSTUP_UPDATE_ROOT:-}" ]; then
            RUST_SOURCES="${RUSTUP_DIST_SERVER:-https://static.rust-lang.org}|$RUSTUP_UPDATE_ROOT"
        else
            RUST_SOURCES="https://static.rust-lang.org|https://static.rust-lang.org/rustup
https://rsproxy.cn|https://rsproxy.cn/rustup
https://mirrors.ustc.edu.cn/rust-static|https://mirrors.ustc.edu.cn/rust-static/rustup"
        fi
        RUSTUP_INIT_OK=""
        while IFS='|' read -r DIST_SERVER UPDATE_ROOT; do
            [ -n "$DIST_SERVER" ] || continue
            RUSTUP_INIT_URL="$UPDATE_ROOT/dist/$RUST_TRIPLE/rustup-init"
            echo "[rust] 下载 $RUSTUP_INIT_URL ..."
            # 低于 50KB/s 持续 20 秒视为卡住，换下一个源
            if curl -L --fail -# --retry 2 --connect-timeout 15 --speed-limit 51200 --speed-time 20 \
                    -o "$RUSTUP_INIT" "$RUSTUP_INIT_URL"; then
                chmod +x "$RUSTUP_INIT"
                export RUSTUP_DIST_SERVER="$DIST_SERVER" RUSTUP_UPDATE_ROOT="$UPDATE_ROOT"
                RUSTUP_INIT_OK=1
                break
            fi
            rm -f "$RUSTUP_INIT"
            echo "[rust] 从 $UPDATE_ROOT 下载失败，尝试下一个源" >&2
        done <<EOF_SOURCES
$RUST_SOURCES
EOF_SOURCES
        if [ -z "$RUSTUP_INIT_OK" ]; then
            echo "[rust] 下载 rustup-init 失败（可设置 RUSTUP_UPDATE_ROOT / RUSTUP_DIST_SERVER 指定镜像后重试）" >&2
            return 1 2>/dev/null || exit 1
        fi
        echo "[rust] 使用源 $RUSTUP_DIST_SERVER"
    fi
    echo "[rust] 首次初始化，执行 rustup-init -y --no-modify-path --default-toolchain $RUST_CHANNEL ..."
    "$RUSTUP_INIT" -y --no-modify-path --profile minimal --default-toolchain "$RUST_CHANNEL" || {
        echo "[rust] rustup-init 失败" >&2
        return 1 2>/dev/null || exit 1
    }
    hash -r
    echo "[rust] 初始化完成"
else
    echo "[rust] rustup 已存在，跳过初始化"
fi

# 按 rust-toolchain.toml 补齐工具链与组件（已安装时很快返回；版本升级后也会自动跟上）
if [ -d "$SRC_TAURI_DIR" ]; then
    (cd "$SRC_TAURI_DIR" && "$RUSTUP_EXE" toolchain install) || {
        echo "[rust] 安装 rust-toolchain.toml 指定的工具链失败" >&2
        return 1 2>/dev/null || exit 1
    }
fi
echo "[rust] 当前 rustc 版本: $(cd "${SRC_TAURI_DIR:-.}" 2>/dev/null; rustc --version)"

# ---------- 8. 进入 src-tauri 安装 Tauri CLI ----------
TAURI_CLI="$CARGO_BIN/cargo-tauri"

if [ ! -x "$TAURI_CLI" ]; then
    if [ ! -d "$SRC_TAURI_DIR" ]; then
        echo "[tauri] 未找到 $SRC_TAURI_DIR，无法安装 Tauri CLI（请先创建 Tauri 项目）" >&2
    else
        echo "[tauri] 未检测到 Tauri CLI，进入 $SRC_TAURI_DIR 并通过 cargo 安装（Tauri v2）..."
        (cd "$SRC_TAURI_DIR" && cargo install tauri-cli --version "^2" --locked) || {
            echo "[tauri] Tauri CLI 安装失败" >&2
            return 1 2>/dev/null || exit 1
        }
        echo "[tauri] Tauri CLI 安装完成"
    fi
else
    echo "[tauri] Tauri CLI 已安装，跳过"
fi
echo "[tauri] 当前 tauri 版本: $(cargo tauri --version)"

# ---------- 9. 汇总 ----------
echo ""
echo "===== 便携环境已就绪 ====="
echo "ROOT        = $ROOT"
echo "AppDir      = $APP_DIR"
echo "FNM_DIR     = $FNM_DIR"
echo "RUSTUP_HOME = $RUSTUP_HOME"
echo "CARGO_HOME  = $CARGO_HOME"
echo "=========================="