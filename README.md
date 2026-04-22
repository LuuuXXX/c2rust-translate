# c2rust-translate

一个使用 c2rust 框架自动化 C 代码到 Rust 代码翻译的工具。

## 版本历史

### v0.4.0（当前版本）
**项目结构重构：模块职责分离 + 目录分组**
- **新增 `build/`**：`builder.rs`（cargo_build / cargo_check）和 `hybrid_build.rs`（混合构建编排）归入同一目录
- **新增 `workflow/`**：`steps.rs`（翻译/验证工作流，原 `workflow.rs`）和 `feature_init.rs` 归入同一目录
- **新增 `translation/`**：`translator.rs`、`verification.rs`、`error_handler.rs`（C→Rust 翻译与错误修复）归入同一目录
- **新增 `ui/`**：`interaction.rs`、`diff_display.rs`、`file_scanner.rs`（用户交互与显示）归入同一目录
- **根目录精简**：仅保留 `main.rs`、`lib.rs`、`util.rs`、`stats.rs`、`progress.rs`、`analyzer.rs`、`git.rs`、`suggestion.rs` 等通用模块
- **删除 `common_tasks.rs`**：薄封装层内联到 `workflow/steps.rs` 对应调用点
- **删除 `initialization.rs`**：重命名为 `workflow/feature_init.rs`，语义更准确

### v0.3.1
**工作流优化：消除冗余步骤**
- **消除重复 `get_config_value` 实现**：删除 `hybrid_build.rs` 中与 `builder.rs` 完全相同的私有实现（~25 行），改用 `crate::builder::get_config_value`
- **减少冗余代码分析更新**：新增 `c2rust_clean_no_analysis`/`c2rust_build_no_analysis`/`c2rust_test_no_analysis` 内部变体，在 clean→build→test 序列中由序列开头统一调用一次 `update_code_analysis`，避免每个步骤各自重复触发（由 3-4 次降至 1 次），涉及 `run_hybrid_build_interactive`、`run_full_build_and_test_interactive`、`complete_file_processing`、`run_final_interval_test_if_needed`
- **修复日志前缀**：`execute_code_warning_check_with_fix_loop` 中代码分析日志缺失 `│ ` 前缀，已与函数内其他输出格式对齐

### v0.3.0
**非致命错误处理改进：**
- **翻译脚本失败非致命化**：`translate_and_fix.py` 非零退出时记录警告并跳过该文件，继续处理下一个文件（新增 `TranslationFailedSignal`，与用户主动跳过的 `SkipFileSignal` 区分）。基础设施错误（找不到项目根目录、无效 feature 名称等）仍为致命错误。
- **修复步骤失败非致命化**：`apply_error_fix`/`apply_warning_fix` 失败时打印警告，本次修复计数为 0，修复循环继续直至达到最大次数
- **git 提交失败非致命化（翻译工作流）**：在每个文件的翻译处理流程（`finalize_file_processing`、`run_final_interval_test_if_needed`）中，提交失败时打印 `⚠ Warning: git commit failed (continuing): ...`，工作树可能保持脏状态，但不中止流程；"✓ Changes committed" 仅在真正有文件被提交时显示（`nothing to commit` 情况不显示）。注意：初始化阶段和代码检查阶段（`initialization.rs`、`common_tasks.rs`）的 git 提交失败仍为致命错误。
- **仍为致命错误的情况**：`code_analyse` 失败、构建失败后用户选择退出、基础设施错误

### v0.2.0
**重大改进：**
- 重构代码结构，提取公共任务模块
- 合并重复的构建函数
- 统一命名规范（去除 `gate_` 前缀）
- 文档全中文化
- 优化用户交互体验

**历史变更：**
- 移除了文件日志功能模块 (`logger` 模块)
- 移除了 `chrono` 依赖

### v0.1.0
- 初始发布版本

## 项目架构

本项目采用模块化设计，每个模块职责单一：

```
src/
├── main.rs                    — CLI 入口（Clap 解析）
├── lib.rs                     — 公共 API（translate_feature / verify_feature）+ 环境变量辅助函数
├── util.rs                    — find_project_root / validate_feature_name（路径工具函数）
├── stats.rs                   — TranslationStats / FileAttemptStat（翻译统计报告）
├── progress.rs                — ProgressState + 显示常量（进度跟踪）
├── analyzer.rs                — code_analyse 外部命令封装
├── git.rs                     — git commit/gc/reflog
├── suggestion.rs              — suggestions.txt 读写
├── build/                     — Rust & 混合构建
│   ├── mod.rs
│   ├── builder.rs             — cargo_build / cargo_check（Rust 编译）
│   └── hybrid_build.rs        — 混合构建序列编排 + get_config_value + c2rust_clean/build/test
├── workflow/                  — 翻译/验证工作流编排
│   ├── mod.rs
│   ├── steps.rs               — 完整的 translate_feature / verify_feature 工作流步骤
│   └── feature_init.rs        — feature 目录初始化与初始化验证
├── translation/               — C→Rust 翻译与修复
│   ├── mod.rs
│   ├── translator.rs          — translate_and_fix.py 脚本调用
│   ├── verification.rs        — 翻译循环中单文件的错误修复循环
│   └── error_handler.rs       — Cargo 错误解析与修复流程
└── ui/                        — 用户交互与显示
    ├── mod.rs
    ├── interaction.rs         — 所有用户交互提示（含文件选择）
    ├── diff_display.rs        — C/Rust 并排代码展示
    └── file_scanner.rs        — .rs 文件磁盘扫描
```

## 功能特性

### 核心功能
- 自动化的 C 到 Rust 翻译工作流
- 支持基于特性(feature)的翻译，使用 `--feature` 标志
- 交互式文件选择或自动处理模式（`--allow-all`）
- 自动初始化 Rust 项目结构
- 自动检测和修复构建错误
- 基于 Git 的版本控制集成
- **公共任务模块** - 标准化的代码检查和构建流程
- **混合构建支持** - 支持 C 和 Rust 代码的混合构建
- **持久化翻译统计** - 支持跨会话的断点续传

### 用户体验优化
- **彩色输出** - 使用不同颜色高亮不同类型的消息
- **代码预览** - 显示正在翻译的 C 代码和构建错误
- **进度追踪** - 实时显示翻译进度
- **交互式修复** - 支持手动修复、跳过文件等多种选项
- **文件选择** - 手动修复时使用上下键选择单个文件，回车确认

## 工作流说明

翻译工作流由 `workflow.rs` 编排，主要步骤如下：

### 1. 代码错误检查阶段
包含以下步骤：
- 执行 cargo build（抑制警告，生成 librust.a 供混合构建链接）
- 执行混合构建序列（clean + build + test，内部统一更新一次代码分析）
- 提交到 git

### 2. 代码告警检查阶段
包含以下步骤：
- 执行 cargo build（显示警告）
- 执行混合构建序列（clean + build + test，内部统一更新一次代码分析）
- 提交到 git

### 3. 混合构建序列
通过 `hybrid_build.rs` 编排：
- 执行清理命令（通过 c2rust-config 获取）
- 执行构建命令（通过 c2rust-config 获取）
- 执行测试命令（通过 c2rust-config 获取）

### 4. 翻译任务
调用 translate_and_fix.py 执行 C 到 Rust 的翻译

## 使用方法

### 基本使用
```bash
# 翻译指定 feature
c2rust-translate translate --feature myfeature

# 自动处理所有文件（不提示）
c2rust-translate translate --feature myfeature --allow-all

# 显示完整输出
c2rust-translate translate --feature myfeature --show-full-output
```

### 工作流程
1. 工具会自动查找项目根目录（包含 `.c2rust` 目录）
2. 如果 feature 目录不存在，会调用 `code_analyse --init` 初始化
3. 执行初始验证（代码错误检查）
4. 扫描待翻译文件（空的 .rs 文件）
5. 选择要翻译的文件（交互式或全选）
6. 对每个文件执行：
   - 翻译 C 代码到 Rust（翻译失败时记录警告并跳过该文件，继续处理下一个文件）
   - 执行代码错误检查（带自动修复循环；修复失败时记录警告并继续）
   - 执行代码告警检查（带自动修复循环；修复失败时记录警告并继续）
   - 提交 git（提交失败时记录警告并继续，工作树可能保持脏状态）
7. 打印翻译统计信息

### 错误处理策略

本 PR 将以下三类失败由致命改为非致命，其余失败类型仍按原有行为处理（可能致命）：

| 失败类型 | 处理方式 |
|----------|----------|
| **翻译脚本失败**（`translate_and_fix.py` 非零退出）| 文件记录到 `translation_failed_files`（区别于用户主动跳过的 `skipped_files`），跳过该文件继续处理下一个；最终统计中单独显示。注意：基础设施错误（找不到项目根目录、无效 feature 名称、无法执行 Python 等）仍为致命错误。 |
| **修复失败**（`apply_error_fix`/`apply_warning_fix` 出错）| 打印警告，本次修复计数为 0，修复循环继续直到达到最大次数 |
| **git 提交失败（翻译工作流）**（`finalize_file_processing`/`run_final_interval_test_if_needed` 中的 `git commit` 出错）| 打印 `⚠ Warning: git commit failed (continuing): ...`，工作树可能保持脏状态，后续提交可能包含额外变更 |
| **git 提交失败（初始化/代码检查阶段）**（`feature_init.rs` 中的 `git commit` 出错）| **致命错误，中止整个流程** |
| **`code_analyse` 失败** | **致命错误，中止整个流程** |
| **构建失败后用户选择退出** | **致命错误，中止整个流程** |
| **基础设施错误**（找不到项目根目录、无效 feature 名等）| **致命错误，中止整个流程** |

## 交互选项说明

在遇到失败时，工具会提供以下选项：

### 编译失败时
- **重试直接翻译**: 清空 .rs 文件，从 C 代码重新翻译
- **添加修复建议**: 输入提示词，让 AI 修改代码
- **手动修复**: 在 VIM 中编辑代码
- **跳过文件**: 跳过当前文件，稍后处理
- **退出**: 中止翻译流程

### 验证失败时
- **手动修复**: 在 VIM 中编辑代码
- **跳过**: 跳过验证，继续流程（会记录警告）
- **退出**: 中止翻译流程

## 手动修复文件选择

当错误涉及多个文件时：
1. 工具会列出所有错误文件
2. 用户使用上下键选择要编辑的单个文件，回车确认
3. 选中文件自动在 VIM 中打开

## 环境变量

| 变量名 | 默认值 | 说明 |
|--------|--------|------|
| `C2RUST_PROCESS_WARNINGS` | 启用 | 设为 `0` 或 `false`（大小写不敏感）可跳过 Phase 2（警告检测与自动修复）；其他任何值或未设置均表示启用 |
| `C2RUST_TEST_CONTINUE_ON_ERROR` | 禁用 | 设为 `1`、`true` 或 `yes`（大小写不敏感）时，`c2rust_test` 失败不会中断流程，仅记录警告并继续执行后续任务。默认情况下（未设置或其他值），测试失败仍为致命错误 |
| `C2RUST_TEST_INTERVAL` | `1` | 设为正整数 `N`，每完成 N 个翻译后执行一次测试。默认值 `1` 表示每次翻译后都执行测试（与现有行为一致）。设为 `0`、非数字或空值时回退为默认值 `1`。**注意**：所有翻译完成后，如果存在自上次测试以来尚未测试的翻译，工具会自动在最后执行一次额外的测试，以确保每次翻译都至少被测试覆盖一次。 |
| `C2RUST_AUTO_RETRY_ON_MAX_FIX` | 禁用 | 设为 `1`、`true` 或 `yes`（大小写不敏感）时，当修复尝试次数达到上限后自动选择重新翻译（RetryDirectly），无需人工干预，保证全自动运行。若已是最后一次翻译机会则自动跳过该文件继续处理后续文件。默认情况下（未设置或其他值）会弹出交互式选择提示。 |

### 示例：忽略测试失败继续执行

```bash
# 测试仍然会运行，但失败结果不会中断翻译流程
export C2RUST_TEST_CONTINUE_ON_ERROR=1
c2rust-translate translate --feature myfeature

# 或者在单次命令中设置
C2RUST_TEST_CONTINUE_ON_ERROR=1 c2rust-translate translate --feature myfeature --allow-all
```

### 示例：每 5 个翻译执行一次测试

```bash
# 每完成 5 个翻译后才执行一次测试，可大幅减少测试次数，加快批量翻译速度
export C2RUST_TEST_INTERVAL=5
c2rust-translate translate --feature myfeature --allow-all

# 或者在单次命令中设置
C2RUST_TEST_INTERVAL=5 c2rust-translate translate --feature myfeature --allow-all
```

### 示例：修复次数耗尽时自动重试（全自动无人值守）

```bash
# 当修复尝试次数达到上限时，自动选择重新翻译，无需人工干预
export C2RUST_AUTO_RETRY_ON_MAX_FIX=1
c2rust-translate translate --feature myfeature --allow-all

# 或者在单次命令中设置
C2RUST_AUTO_RETRY_ON_MAX_FIX=1 c2rust-translate translate --feature myfeature --allow-all
```

## 依赖要求

- Rust 工具链 (rustc, cargo)
- Python 3.x
- code_analyse 工具
- c2rust-config 工具
- translate_and_fix.py 脚本

## 开发

### 运行测试
```bash
cargo test
```

### 代码格式化
```bash
cargo fmt
```

### Lint 检查
```bash
cargo clippy
```

## 贡献指南

欢迎贡献！请遵循以下步骤：

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 打开 Pull Request

## 联系方式

如有问题或建议，请提交 Issue。
