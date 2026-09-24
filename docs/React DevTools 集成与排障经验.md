# React DevTools 集成与排障经验

**适用版本：FileDock 1.0.0**

记录在 FileDock（Tauri + React）项目中集成 `react-devtools` 时遇到的问题及解决方案，供后续开发参考。

---

## 目录

1. [集成方案](#1-集成方案)
2. [典型问题：Electron 二进制缺失](#2-典型问题electron-二进制缺失)
3. [修复方法（Windows）](#3-修复方法windows)
4. [根因分析](#4-根因分析)
5. [验证](#5-验证)
6. [排障速查](#6-排障速查)
7. [macOS 补充：镜像下载卡死与手动安装](#7-macos-补充镜像下载卡死与手动安装)
8. [坑：`npm install electron` 可能顺带丢失 `devtools` script](#8-坑npm-install-electron-可能顺带丢失-devtools-script)

---

## 1. 集成方案

FileDock 的 `react-devtools` 不通过独立命令启动，而是通过 Vite 插件内建到 `vite.config.ts` 的 `reactDevtools()` 中：

- 运行 `tauri dev` 时，Vite 启动阶段自动 spawn `node_modules/react-devtools/bin.js`，监听 `:8097`。
- 通过 `transformIndexHtml` 在 HTML 头部前置注入 `<script src="http://localhost:8097">` 桥接脚本，让 Tauri 的 WebView2 与独立 DevTools 窗口通信。
- 可用环境变量 `RDT=false` 禁用该桥接。

因此 **无需单独运行 `npm run devtools`**，`npm run tauri dev` 一条命令即可同时拉起前端、Rust 后端与 React DevTools 窗口。`npm run devtools`（即 `react-devtools` 独立命令）仍保留在 `package.json` 的 `scripts` 中，用于单独调试桥接是否正常。

---

## 2. 典型问题：Electron 二进制缺失

### 现象

运行 `tauri dev` 后，Vite 与 Rust 均正常启动，`filedock.exe`（或 macOS 下的 `.app`）也能跑起来，但日志中出现：

```
D:\...\node_modules\electron\index.js:17
    throw new Error('Electron failed to install correctly, please delete node_modules/electron and try installing again');
    ^

Error: Electron failed to install correctly, please delete node_modules/electron and try installing again
```

随后：

```
react-devtools not reachable on :8097 (bridge inactive)
```

即 React DevTools 桥接未激活，独立窗口不弹出。

### 触发条件

`react-devtools` 的独立窗口运行在 Electron 上。当 `node_modules/electron` 的二进制文件缺失时，`react-devtools` 启动失败：

1. `react-devtools/bin.js` 加载时 `require('electron')`。
2. `electron/index.js` 的 `getElectronPath()` 读取 `path.txt`、校验 `dist/<exe>` 是否存在。
3. 二进制缺失 → 触发 `install.js` 尝试下载 → 下载失败 → 抛出上述错误。

---

## 3. 修复方法（Windows）

在 `app/` 目录下重装 electron 二进制：

```powershell
cd app
npm install electron --no-save
```

说明：

- `npm install electron` 会重新拉取 electron 包并触发 `postinstall` 脚本（`install.js`），从 GitHub release 下载对应平台的 Electron 二进制到 `node_modules/electron/dist/`，并写入 `path.txt`。
- `--no-save` 让 electron 仅作为临时 dev 依赖，不写入 `package.json`（因为 `react-devtools` 已间接声明它）。
- 下载成功后，`electron/index.js` 的校验通过，`react-devtools` 能正常启动。

然后重启开发服务器：

```powershell
./app/scripts/dev.ps1
```

---

## 4. 根因分析

Electron 包的 JS 包装器（`node_modules/electron/index.js`）在运行时按以下顺序查找二进制：

1. 读取 `path.txt`（记录可执行文件名，如 `electron.exe` 或 `Electron.app/Contents/MacOS/Electron`）。
2. 校验 `dist/<exe>` 是否存在。
3. 若缺失，调用 `downloadElectron()` 下载。

之前 `node_modules/electron` 是旧版本包，其 `postinstall` 安装阶段从未成功下载平台二进制（`dist/` 为空、`path.txt` 缺失），导致运行时校验失败、下载也失败。

`npm install electron` 同时完成两件事：

1. 替换 `node_modules/electron` 为新版本包文件；
2. 执行 `postinstall` 补齐 `dist/` 下的二进制与 `path.txt`。

核心：**不是改代码，而是补上 electron 本该在安装阶段下载却缺失的二进制文件。**

---

## 5. 验证

修复后重启 `tauri dev`，日志应出现：

```
react-devtools ready on :8097
```

同时：

- Vite 前端：`http://localhost:1420/` ✓
- Rust 后端：应用进程运行 ✓
- React DevTools：`:8097` 桥接激活，独立窗口自动弹出并连接 WebView ✓

也可以单独验证桥接端口是否可达（无需启动完整 `tauri dev`）：

```bash
cd app
npm run devtools &          # 启动独立 React DevTools 窗口/服务
curl -fsS http://localhost:8097 | head -c 80   # 应返回桥接脚本内容（非连接失败）
```

---

## 6. 排障速查

| 现象 | 可能原因 | 处理 |
|---|---|---|
| `Electron failed to install correctly` | electron 二进制未下载 | `cd app && npm install electron --no-save` |
| `react-devtools not reachable on :8097` | react-devtools 未启动（常因上一条） | 修复 electron 后重启 `tauri dev` |
| DevTools 窗口弹出但无连接 | 桥接脚本未注入 | 确认 `vite.config.ts` 的 `reactDevtools()` 插件存在；检查是否被 `RDT=false` 禁用 |
| 下载 electron 超时 | 网络问题 / GitHub 访问受限 | 设置代理后重试，或用 `ELECTRON_MIRROR` 指定镜像源 |
| `path.txt` 存在但 `dist/` 为空 | 旧版本残留 | 删除 `node_modules/electron` 后重装 |
| `npm install electron`/`npm rebuild electron` 长时间卡在 "Downloading Electron binary..." | 官方下载源（GitHub Releases）不可达，`ELECTRON_MIRROR` 也可能被 `@electron/get` 内部逻辑忽略 | 见 [第 7 节](#7-macos-补充镜像下载卡死与手动安装) 手动下载解压 |
| `npm run devtools` 报 `Missing script: "devtools"` | `package.json` 的 `scripts.devtools` 被意外移除（常见于误操作或工具改写 package.json） | 见 [第 8 节](#8-坑npm-install-electron-可能顺带丢失-devtools-script) |

### 临时禁用 DevTools

如需临时关闭 React DevTools 桥接（例如排查启动性能）：

```powershell
$env:RDT="false"; ./app/scripts/dev.ps1
```

```bash
# macOS/Linux
RDT=false ./app/scripts/dev.sh
```

---

## 7. macOS 补充：镜像下载卡死与手动安装

### 现象

在 macOS（Apple Silicon，`darwin arm64`）上执行：

```bash
cd app
npm install electron --no-save
```

或设置镜像后重试：

```bash
ELECTRON_MIRROR=https://npmmirror.com/mirrors/electron/ npm install electron --no-save
```

或：

```bash
npm rebuild electron --foreground-scripts
```

进程会长时间卡在 `Downloading Electron binary...`，既不报错也不退出（在部分网络/沙箱环境下可能持续数分钟甚至更久）。此时 `node_modules/electron/dist/` 仍为空，`path.txt` 缺失。

### 原因

- `electron` 的 `postinstall`（`install.js`）通过 `@electron/get` 的 `downloadArtifact()` 发起下载，该请求可能因网络策略（代理、沙箱环境限制）被静默挂起，而不是快速失败。
- `ELECTRON_MIRROR` 环境变量在部分 `@electron/get` 版本 / 网络环境下不一定生效，即使镜像域名本身可以 `curl` 通。

### 手动修复步骤

1. 确认镜像域名本身是否可达（可绕过 npm 的下载逻辑，直接用 `curl` 验证）：

   ```bash
   curl -IL --max-time 20 \
     https://cdn.npmmirror.com/binaries/electron/<version>/electron-v<version>-darwin-arm64.zip
   ```

   `<version>` 取 `node_modules/electron/package.json` 中的 `version` 字段（例如 `23.3.13`）。

2. 若可达但 `npm install`/`npm rebuild` 仍卡死，改为手动下载并解压到 electron 包目录：

   ```bash
   cd app
   VERSION=$(node -p "require('./node_modules/electron/package.json').version")
   curl -L --fail --retry 2 --connect-timeout 20 --max-time 600 \
     -o /tmp/electron.zip \
     "https://cdn.npmmirror.com/binaries/electron/${VERSION}/electron-v${VERSION}-darwin-arm64.zip"

   rm -rf node_modules/electron/dist
   mkdir -p node_modules/electron/dist
   unzip -q /tmp/electron.zip -d node_modules/electron/dist
   rm -f /tmp/electron.zip

   # 写入 path.txt（注意：不能有末尾换行，否则 index.js 的路径拼接会失败）
   printf 'Electron.app/Contents/MacOS/Electron' > node_modules/electron/path.txt

   # 写入 version 文件，避免 install.js 的 isInstalled() 校验误判需要重新下载
   printf "v${VERSION}" > node_modules/electron/dist/version
   ```

3. 验证 `require('electron')` 能正确解析到可执行文件路径：

   ```bash
   node -e "console.log(require('electron'))"
   # 期望输出类似：
   # /path/to/app/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron
   ```

4. 启动 `npm run devtools`，确认 Electron 进程能正常拉起（`ps` 应能看到 `Electron.app/.../MacOS/Electron` 及其 Helper 子进程），且 `http://localhost:8097` 可访问。

### 注意事项

- `path.txt` **不能包含换行符**：`install.js` 正常写入时用的是 `fs.promises.writeFile(path, platformPath)`（无换行），若手动用 `echo`/`printf '...\n'` 写入会导致 `path.join(__dirname, 'dist', executablePath)` 拼出带换行的路径，进而 `fs.existsSync` 判断失败并再次触发下载。**务必用 `printf '...'`（无 `\n`）而不是 `echo`。**
- 不同平台/架构对应的产物文件名不同：`darwin-arm64`、`darwin-x64`、`linux-x64`、`win32-x64` 等，`platformPath`（即 `path.txt` 内容）也不同（`Electron.app/Contents/MacOS/Electron` / `electron` / `electron.exe`），照抄本节命令前请确认目标平台。
- `react-devtools` 自身在 `node_modules/react-devtools/node_modules/electron` 下也声明了一份 electron 依赖（版本可能与顶层 `devDependencies` 中的不同）。如果只修复了顶层 `node_modules/electron` 而 `npm run devtools` 仍报错，需要对 `node_modules/react-devtools/node_modules/electron` 重复本节步骤。

---

## 8. 坑：`npm install electron` 可能顺带丢失 `devtools` script

### 现象

按第 3/7 节修复 Electron 二进制后，再执行：

```bash
npm run devtools
```

报错：

```
npm error Missing script: "devtools"
```

但 `app/package.json` 的 `devDependencies` 中明明还保留着 `"react-devtools": "^8.0.0"`。

### 原因

`npm install electron --no-save` 会重写并规范化 `package.json`/`package-lock.json`（即便加了 `--no-save`，npm 仍可能重排/精简 `scripts` 字段的格式化输出，取决于 npm 版本和锁文件状态）。这次排障过程中，`scripts.devtools` 字段在一次 `npm install electron --no-save` 执行后从 `package.json` 中消失了。

### 处理

修复后务必 `git diff app/package.json`，确认除了预期改动外没有意外丢失字段；若 `devtools` script 丢失，手动补回：

```json
{
  "scripts": {
    "devtools": "react-devtools"
  }
}
```

### 建议

- 每次执行 `npm install <pkg> --no-save`（尤其是为了修复二进制而非真正变更依赖）后，用 `git diff` 检查 `package.json`/`package-lock.json` 的意外改动，必要时 `git checkout -- app/package-lock.json` 还原锁文件（`--no-save` 通常不需要保留锁文件里的临时改动）。
- 若锁文件被写入了不需要的临时改动（例如仅为了下载二进制而 `npm install electron`），修复完成后可执行 `git checkout -- app/package-lock.json` 复原，只保留 `node_modules` 里的二进制文件本身。
