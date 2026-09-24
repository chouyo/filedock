# React DevTools 集成与排障经验

**适用版本：FileDock 1.0.0**

记录在 FileDock（Tauri + React）项目中集成 `react-devtools` 时遇到的问题及解决方案，供后续开发参考。

---

## 目录

1. [集成方案](#1-集成方案)
2. [典型问题：Electron 二进制缺失](#2-典型问题electron-二进制缺失)
3. [修复方法](#3-修复方法)
4. [根因分析](#4-根因分析)
5. [验证](#5-验证)
6. [排障速查](#6-排障速查)

---

## 1. 集成方案

FileDock 的 `react-devtools` 不通过独立命令启动，而是通过 Vite 插件内建到 `vite.config.ts` 的 `reactDevtools()` 中：

- 运行 `tauri dev` 时，Vite 启动阶段自动 spawn `node_modules/react-devtools/bin.js`，监听 `:8097`。
- 通过 `transformIndexHtml` 在 HTML 头部前置注入 `<script src="http://localhost:8097">` 桥接脚本，让 Tauri 的 WebView2 与独立 DevTools 窗口通信。
- 可用环境变量 `RDT=false` 禁用该桥接。

因此 **无需单独运行 `npm run devtools`**，`npm run tauri dev` 一条命令即可同时拉起前端、Rust 后端与 React DevTools 窗口。

---

## 2. 典型问题：Electron 二进制缺失

### 现象

运行 `tauri dev` 后，Vite 与 Rust 均正常启动，`filedock.exe` 也能跑起来，但日志中出现：

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

## 3. 修复方法

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

1. 读取 `path.txt`（记录可执行文件名，如 `electron.exe`）。
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
- Rust 后端：`filedock.exe` 运行 ✓
- React DevTools：`:8097` 桥接激活，独立窗口自动弹出并连接 WebView2 ✓

---

## 6. 排障速查

| 现象 | 可能原因 | 处理 |
|---|---|---|
| `Electron failed to install correctly` | electron 二进制未下载 | `cd app && npm install electron --no-save` |
| `react-devtools not reachable on :8097` | react-devtools 未启动（常因上一条） | 修复 electron 后重启 `tauri dev` |
| DevTools 窗口弹出但无连接 | 桥接脚本未注入 | 确认 `vite.config.ts` 的 `reactDevtools()` 插件存在；检查是否被 `RDT=false` 禁用 |
| 下载 electron 超时 | 网络问题 / GitHub 访问受限 | 设置代理后重试，或用 `ELECTRON_MIRROR` 指定镜像源 |
| `path.txt` 存在但 `dist/` 为空 | 旧版本残留 | 删除 `node_modules/electron` 后重装 |

### 临时禁用 DevTools

如需临时关闭 React DevTools 桥接（例如排查启动性能）：

```powershell
$env:RDT="false"; ./app/scripts/dev.ps1
```