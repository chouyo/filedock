#!/usr/bin/env bash
# ============================================================
# 便携式 Tauri 开发环境配置脚本（macOS 版）
# 用法：在终端中执行   source ./setup-env.sh
# 特点：所有路径基于脚本所在目录推导，整个文件夹可任意移动
#       自动读取 app/.node-version，安装并启用对应 Node
#       在 app 目录执行 npm install（npm 源见 app/.npmrc）
#       自动初始化 Rust 工具链
#       自动进入 src-tauri 安装 Tauri CLI
# ============================================================

# 该脚本通过 source 执行，不使用 set -e（否则任一命令失败都会关闭当前终端）；
# 关键步骤失败时显式 return

# ---------- 0. 项目目录 ----------
# 默认取脚本同级的 app 目录；如需改，直接改这一行
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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

eval "$("$FNM_EXE" env --use-on-cd --shell bash)"

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
    (cd "$APP_DIR" && npm install) || {
        echo "[npm] npm install 失败" >&2
        return 1 2>/dev/null || exit 1
    }
    echo "[npm] npm install 完成"
else
    echo "[npm] 未找到 $APP_DIR，跳过 npm install" >&2
fi

# ---------- 7. 初始化 Rust 工具链 ----------
RUSTUP_INIT="$TOOLS_DIR/rustup-init"
CARGO_EXE="$CARGO_BIN/cargo"

if [ ! -x "$CARGO_EXE" ]; then
    if [ ! -x "$RUSTUP_INIT" ]; then
        # 如果系统已装 rustup，可直接使用
        if command -v rustup >/dev/null 2>&1; then
            echo "[rust] 使用系统已安装的 rustup"
        else
            echo "[rust] 未找到 $RUSTUP_INIT，请把 rustup-init 放到 tools/ 目录下" >&2
            return 1 2>/dev/null || exit 1
        fi
    else
        echo "[rust] 首次初始化，执行 rustup-init -y --no-modify-path ..."
        "$RUSTUP_INIT" -y --no-modify-path || {
            echo "[rust] rustup-init 失败" >&2
            return 1 2>/dev/null || exit 1
        }
        echo "[rust] 初始化完成"
    fi
else
    echo "[rust] Rust 工具链已存在，跳过初始化"
fi
echo "[rust] 当前 rustc 版本: $(rustc --version)"

# ---------- 8. 进入 src-tauri 安装 Tauri CLI ----------
TAURI_CLI="$CARGO_BIN/cargo-tauri"
SRC_TAURI_DIR="$APP_DIR/src-tauri"

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