# 文件搜索方案基准测试

对比几种文件索引 / 搜索方案在 FileDock 场景下的表现，为 [文件搜索性能优化方案](../../../docs/文件搜索性能优化方案.md) 的选型提供实测数据。

所有程序都是 Cargo example，只在编译 example 时才会用到这些依赖（`[dev-dependencies]`），**不会进入 app 安装包**。

## 参与对比的方案

| example | 方案 | 说明 |
|---|---|---|
| `bench_current` | 基线：app 现状 | 直接调用 app 的 `scan.rs`，每次列表、搜索、刷新都全量遍历 |
| `bench_seekr` | 方案 1：seekr | 通过公开 API 使用 [seekr](https://github.com/muhammad-fiaz/seekr)（固定在 `f11bc11`），不改它的源码；它缺的能力由标注了 `GLUE:` 的胶水代码补上 |
| `bench_native` | 方案 2：walkdir/ignore + notify + SQLite | 方案文档第 4、5 节的推荐架构：并行遍历、规范化表结构、watcher 增量写回、重启后按目录修改时间对账 |
| `bench_spotlight` | 方案 3：macOS Spotlight | 通过 CoreServices `MDQuery` 查询系统索引；**仅 macOS** |

辅助程序：`bench_dataset`（生成 / 修改 / 还原 / 删除数据集）、`bench_compare`（汇总对比表）。

## 目录结构

```
examples/
├── bench_rules.json      测试用的 5 个分类（glob 递归、glob 非递归、正则、全量）和搜索词
├── bench_common/         共用代码（没有 main.rs，Cargo 不会把它当成 example）
├── bench_dataset/        数据集工具
├── bench_current/        基线
├── bench_seekr/          方案 1
├── bench_native/         方案 2
├── bench_spotlight/      方案 3
├── bench_compare/        对比表
└── scripts/              run-all.sh（macOS / Linux）、run-all.ps1（Windows）
```

规则匹配和"标准答案"直接用 `#[path]` 引入 app 的 `matcher.rs`、`scan.rs` 等源码，**与 app 行为完全一致**，不存在复制后走样的问题。

## 前提

- rustc ≥ 1.88（seekr 使用 edition 2024 和 let-chains）。
- 首次编译需要联网，从 GitHub 拉取 seekr。
- 首次编译会顺带编译 app 本身（Tauri），需要几分钟。

## 快速开始

在 `app/src-tauri` 目录下：

```bash
./examples/scripts/run-all.sh 100k
```

```powershell
.\examples\scripts\run-all.ps1 -Size 100k
```

脚本会生成数据集，依次跑完所有方案（macOS 上先跑 Spotlight），最后打印对比表，并写入 `~/FileDockBench/results/compare-<数据集>.md`。其他参数会原样传给每个方案，例如：

```bash
./examples/scripts/run-all.sh 1m --repeat 3 --timeout 120
```

## 单独运行

```bash
cargo run --release --example bench_dataset -- gen --size 100k          # 输出数据集名 100k-seed42
cargo run --release --example bench_native -- --dataset 100k-seed42    # 默认跑 s1,s2,s3,s4,s6
cargo run --release --example bench_dataset -- mutate --dataset 100k-seed42
cargo run --release --example bench_native -- --dataset 100k-seed42 --scenario s5,s6
cargo run --release --example bench_dataset -- revert --dataset 100k-seed42
cargo run --release --example bench_compare -- --dataset 100k-seed42
```

每个方案都支持 `--help`。各自的特有参数：

| example | 参数 |
|---|---|
| `bench_seekr` | `--mode app\|indexer`（是否经过 `SeekrApp`，它每次索引后都会重建语义编码器）、`--no-cache` |
| `bench_native` | `--walker parallel\|walkdir`、`--threads N`、`--fts`（trigram 全文索引）、`--resume-mode dirs\|full`（第 2 级 / 第 3 级对账）、`--debounce-ms` |
| `bench_spotlight` | `--index-timeout`、`--poll-secs`、`--stable-polls`（S1 等待 Spotlight 索引的参数） |

## 测试场景

| 场景 | 测什么 | 对应的问题 |
|---|---|---|
| S1 | 从零建立索引的耗时、内存峰值、索引占用的磁盘 | 首次成本 |
| S2 | 每个分类的文件数和第一屏（100 条，按修改时间倒序），取中位数 | 切换分类慢 |
| S3 | 分类内按文件名子串搜索，取各搜索词中位数 | 搜索框 |
| S4 | 监听状态下新建 500 个文件、改 / 重命名 / 删、删除整个目录后，索引多久能反映出来 | 文件变化后全量重扫 |
| S5 | 进程退出后离线修改 1% 的文件，重新启动到结果一致的耗时 | 重启后重新索引 |
| S6 | 与 app 当前扫描结果（`scan.rs`）逐个比对：缺少、多出、大小过期 | 正确性 |

S5 需要"退出 → 离线修改 → 重新启动"，所以要分两次运行，中间执行 `bench_dataset mutate`；跑完用 `revert` 还原，下一个方案才能从同样的数据开始。`run-all` 脚本会自动处理。

S5 的三个一致性指标：

- **文件集合一致**：文件一个不缺、一个不多。
- **可见文件完全一致**：忽略隐藏目录（如 `.git`）里的文件后，集合和大小都一致。专门用来衡量 Spotlight，它从不索引隐藏目录。
- **完全一致**：集合和大小都一致。

## 数据位置

都在仓库之外，默认是 `~/FileDockBench`，可以用环境变量 `FILEDOCK_BENCH_HOME` 改：

```
~/FileDockBench/
├── datasets/<数据集>/                 数据集本身
├── datasets/<数据集>.manifest.json    数据集信息
├── datasets/<数据集>.mutations.json   未还原的离线修改（存在时说明需要 revert）
├── state/<方案>/<数据集>/              各方案的持久化索引
└── results/                          每次运行的 JSON 报告和 compare-*.md
```

放在仓库外的原因：数据集动辄上百万个文件，放进仓库会拖垮 git 和编辑器；写进 `src-tauri/` 还会触发 `tauri dev` 重新编译。

数据集规模参考：大约 75% 的文件是空文件，其余 1～1024 字节。100 万个文件大约占几百 MB（APFS / NTFS 每个非空文件至少占一个簇）。

## 冷缓存测试

默认测的是热缓存。要测"刚开机"的情况：

- macOS：每次运行前执行 `sudo purge`。
- Windows：重启，或用 Sysinternals RAMMap 执行 "Empty Standby List"。

## Spotlight 注意事项

- 数据集生成后，Spotlight 会在后台索引它，这会占用 CPU 和磁盘，所以 `run-all.sh` **先跑 Spotlight**，它的 S1 衡量的正是系统追上新数据所需的时间。对于已经被索引过的数据集，S1 很快就结束。
- Spotlight **从不索引隐藏目录**（数据集里的 `.git`），S6 会如实显示这部分缺失（报告里的 `missingInHiddenDirs`）。
- `/tmp` 不在 Spotlight 的索引范围内，所以数据集必须放在用户目录下。
- 测完后，如果不希望 Spotlight 继续保留这些文件：先 `bench_dataset clean` 删除数据集；也可以在"系统设置 → Spotlight → 隐私"里把 `~/FileDockBench` 排除。

## 结果解读提示

- `native` 的第 2 级对账（默认）在 S5 之后会有少量"大小旧"：这是设计中已知的局限。只改文件内容不会改变目录的修改时间，所以这些文件没有被重新读取。`--resume-mode full`（第 3 级）没有这个问题，但更慢。
- `seekr` 在 S5 之后会缺少被重命名的文件：它的 `index_incremental` 只收录修改时间比上次索引新的文件，而重命名不改变修改时间。
- `seekr` 的 S2、S4、S5 包含胶水代码的开销，报告的 `notes` 里有说明。
- `current` 没有索引，S1、S5 显示"无索引"；它的 S4 是一次完整扫描的耗时（app 收到变化事件后会重新扫描整个分类）。

## 清理

```bash
cargo run --release --example bench_dataset -- clean --dataset 100k-seed42   # 删除数据集及所有方案的索引
rm -rf ~/FileDockBench                                                        # 全部删除
```
