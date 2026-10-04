# ============================================================
# 便携式 Tauri 开发环境配置脚本
# 用法：在 PowerShell 中点源执行   . .\setup-env.ps1
# 特点：所有路径基于脚本所在目录推导，整个文件夹可任意移动
#       自动读取 app\.node-version，安装并启用对应 Node
#       在 app 目录执行 npm install（npm 源见 app\.npmrc）
#       补齐 react-devtools 所需的 Electron 二进制（缺失时从镜像下载）
#       自动初始化 Rust 工具链
#       自动进入 src-tauri 安装 Tauri CLI
# ============================================================

# 点源执行会在调用方作用域运行，结束时恢复原来的 ErrorActionPreference
$__setupEnvPrevEAP = $ErrorActionPreference
$ErrorActionPreference = 'Stop'
try {

    # ---------- 0. 项目目录（不用 param，改普通变量，避免位置限制） ----------
    # 默认取脚本同级的 app 目录；如需改，直接改这一行
    $AppDir = Join-Path $PSScriptRoot "app"

    # ---------- 1. 以脚本所在目录为根 ----------
    $ROOT = $PSScriptRoot

    # ---------- 2. 推导子目录 ----------
    $TOOLS_DIR   = Join-Path $ROOT "tools"
    $SDK_DIR     = Join-Path $ROOT "sdk"
    $FNM_DIR     = Join-Path $SDK_DIR "fnm"
    $RUSTUP_HOME = Join-Path $SDK_DIR "rustup"
    $CARGO_HOME  = Join-Path $SDK_DIR "cargo"
    $CARGO_BIN   = Join-Path $CARGO_HOME "bin"

    # ---------- 3. 确保目录存在 ----------
    foreach ($d in @($TOOLS_DIR, $SDK_DIR, $FNM_DIR, $RUSTUP_HOME, $CARGO_HOME)) {
        if (-not (Test-Path $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null }
    }

    # ---------- 4. 导出环境变量（仅当前会话） ----------
    $env:FNM_DIR     = $FNM_DIR
    $env:RUSTUP_HOME = $RUSTUP_HOME
    $env:CARGO_HOME  = $CARGO_HOME

    foreach ($p in @($TOOLS_DIR, $CARGO_BIN)) {
        $parts = $env:PATH -split ';'
        if ($parts -notcontains $p) { $env:PATH = "$p;$env:PATH" }
    }

    # ---------- 5. 初始化 fnm ----------
    $fnmExe = Join-Path $TOOLS_DIR "fnm.exe"
    if (-not (Test-Path $fnmExe)) {
        throw "[fnm] 未找到 $fnmExe，请把 fnm.exe 放到 tools\ 目录下"
    }
    & $fnmExe env --use-on-cd --shell powershell | Out-String | Invoke-Expression

    # ---------- 6. 从 app\.node-version 读取并安装/启用 Node ----------
    $nodeVerFile = Join-Path $AppDir ".node-version"
    if (Test-Path $nodeVerFile) {
        $nodeVer = (Get-Content $nodeVerFile -Raw).Trim()
        if ([string]::IsNullOrWhiteSpace($nodeVer)) {
            Write-Warning "[node] $nodeVerFile 内容为空，跳过"
        } else {
            Write-Host "[node] 读取版本: $nodeVer  (来自 $nodeVerFile)"

            Write-Host "[node] 执行 fnm install $nodeVer ..."
            & $fnmExe install $nodeVer

            Write-Host "[node] 执行 fnm use $nodeVer ..."
            & $fnmExe use $nodeVer

            Write-Host "[node] 当前 node 版本: $(& node --version)"
        }
    } else {
        Write-Warning "[node] 未找到 $nodeVerFile，跳过 Node 安装"
    }

    # ---------- 6.5 安装依赖（npm 源由 app\.npmrc 指定） ----------
    if (Test-Path $AppDir) {
        Write-Host "[npm] 进入 $AppDir 执行 npm install ..."
        Push-Location $AppDir
        try {
            & npm install
            Write-Host "[npm] npm install 完成"
        } finally {
            Pop-Location
        }
    } else {
        Write-Warning "[npm] 未找到 $AppDir，跳过 npm install"
    }

    # ---------- 6.6 补齐 react-devtools 所需的 Electron 二进制 ----------
    # 详见 docs\React DevTools 集成与排障经验.md。electron 的 postinstall 可能未下载成功
    # （dist\ 为空、path.txt 缺失），导致 react-devtools 无法启动。
    # 不用 `npm install electron --no-save`：它会装最新版 electron（与锁文件不符，
    # 且新版 install.js 在当前 Node 下报 ERR_REQUIRE_ESM）。这里按锁定版本从镜像手动下载解压。
    if (Test-Path (Join-Path $AppDir "node_modules\react-devtools")) {
        Push-Location $AppDir
        try {
            # 按 react-devtools 的解析路径定位 electron（可能是顶层或嵌套的那份）
            $electronDir = (& node -p "const p=require('path');p.dirname(require.resolve('electron/package.json',{paths:[p.dirname(require.resolve('react-devtools/package.json'))]}))").Trim()
            # 不能用 2>$null：PS 5.1 + ErrorActionPreference=Stop 下原生命令的 stderr 会变成终止错误
            & node -e "try{require(process.argv[1]);process.exit(0)}catch(e){process.exit(1)}" $electronDir
            if ($LASTEXITCODE -eq 0) {
                Write-Host "[electron] 二进制已就绪，跳过"
            } else {
                $ver  = (& node -p "require(process.argv[1] + '/package.json').version" $electronDir).Trim()
                $arch = (& node -p "process.arch").Trim()
                $url  = "https://cdn.npmmirror.com/binaries/electron/$ver/electron-v$ver-win32-$arch.zip"
                $zip  = Join-Path $env:TEMP "electron-v$ver-win32-$arch.zip"
                $dist = Join-Path $electronDir "dist"
                Write-Host "[electron] 二进制缺失，下载 $url ..."
                & curl.exe -L --fail --retry 2 --connect-timeout 20 --max-time 600 -o $zip $url
                if ($LASTEXITCODE -ne 0) { throw "下载失败: $url" }
                if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
                Expand-Archive -Path $zip -DestinationPath $dist -Force
                Remove-Item -Force $zip
                # path.txt 不能有末尾换行；version 文件让 install.js 认为已安装
                [IO.File]::WriteAllText((Join-Path $electronDir "path.txt"), "electron.exe")
                [IO.File]::WriteAllText((Join-Path $dist "version"), "v$ver")
                Write-Host "[electron] 已安装 Electron $ver 到 $dist"
            }
        } catch {
            Write-Warning "[electron] 补齐 Electron 失败（React DevTools 将不可用）: $_"
        } finally {
            Pop-Location
        }
    }

    # ---------- 7. 初始化 Rust 工具链 ----------
    $rustupInit = Join-Path $TOOLS_DIR "rustup-init.exe"
    $cargoExe   = Join-Path $CARGO_BIN "cargo.exe"

    if (-not (Test-Path $cargoExe)) {
        if (-not (Test-Path $rustupInit)) {
            throw "[rust] 未找到 $rustupInit，请把 rustup-init.exe 放到 tools\ 目录下"
        }
        Write-Host "[rust] 首次初始化，执行 rustup-init.exe -y --no-modify-path ..."
        & $rustupInit -y --no-modify-path
        Write-Host "[rust] 初始化完成"
    } else {
        Write-Host "[rust] Rust 工具链已存在，跳过初始化"
    }
    Write-Host "[rust] 当前 rustc 版本: $(& rustc --version)"

    # ---------- 8. 进入 src-tauri 安装 Tauri CLI ----------
    $tauriCliExe = Join-Path $CARGO_BIN "cargo-tauri.exe"
    $srcTauriDir = Join-Path $AppDir "src-tauri"

    if (-not (Test-Path $tauriCliExe)) {
        if (-not (Test-Path $srcTauriDir)) {
            Write-Warning "[tauri] 未找到 $srcTauriDir，无法安装 Tauri CLI（请先创建 Tauri 项目）"
        } else {
            Write-Host "[tauri] 未检测到 Tauri CLI，进入 $srcTauriDir 并通过 cargo 安装（Tauri v2）..."
            Push-Location $srcTauriDir
            try {
                & cargo install tauri-cli --version "^2" --locked
                Write-Host "[tauri] Tauri CLI 安装完成"
            } finally {
                Pop-Location
            }
        }
    } else {
        Write-Host "[tauri] Tauri CLI 已安装，跳过"
    }
    Write-Host "[tauri] 当前 tauri 版本: $(& cargo tauri --version)"

    # ---------- 9. 汇总 ----------
    Write-Host ""
    Write-Host "===== 便携环境已就绪 ====="
    Write-Host "ROOT        = $ROOT"
    Write-Host "AppDir      = $AppDir"
    Write-Host "FNM_DIR     = $FNM_DIR"
    Write-Host "RUSTUP_HOME = $RUSTUP_HOME"
    Write-Host "CARGO_HOME  = $CARGO_HOME"
    Write-Host "=========================="
} finally {
    $ErrorActionPreference = $__setupEnvPrevEAP
    Remove-Variable __setupEnvPrevEAP
}