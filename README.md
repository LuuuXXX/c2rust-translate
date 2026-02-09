# c2rust-translate

一个使用 c2rust 框架自动化 C 代码到 Rust 代码翻译的工具。

## 功能特性

### 核心功能
- 自动化的 C 到 Rust 翻译工作流
- 支持基于特性(feature)的翻译，使用 `--feature` 标志
- **目标制品选择** - 在翻译前自动提示选择目标制品（从 `.c2rust/<feature>/c/targets.list` 读取）
- 交互式文件选择或自动处理模式（`--allow-all`）
- 自动初始化 Rust 项目结构
- 自动检测和修复构建错误
- 基于 Git 的版本控制集成

### 用户体验优化
- **彩色输出** - 构建/测试/清理命令使用不同颜色高亮，成功/错误/警告消息带有图标标识
- **代码预览** - 显示正在翻译的 C 代码（默认前 15 行）和构建错误（默认前 10 行），可使用 `--show-full-output` 选项显示完整内容
- **进度跟踪** - 基于文件内容实时跟踪翻译进度，显示当前处理文件在所有文件中的序号和总数（如 `[7/10] Processing var_example.rs`，表示已处理6个，正在处理第7个，共10个文件）
- **智能恢复** - 中断后重新运行时，自动跳过已处理（非空）的文件，进度序号会继续累计（例如已完成6个文件后中断，重启后会从 `[7/10]` 继续）
- **执行时间统计** - 显示所有命令的耗时，便于识别性能瓶颈
- **文件排序显示** - 未处理文件按字母顺序列出

详细的用户体验改进说明请参见 [用户体验改进.md](用户体验改进.md)。

## 安装

从源码构建：

```bash
cargo build --release
```

二进制文件将位于 `target/release/c2rust-translate`。

## 使用方法

### 翻译一个特性

```bash
# 翻译指定的特性（交互模式：会提示选择要处理的文件）
c2rust-translate translate --feature <特性名称>

# 翻译默认特性（不指定 --feature 时默认使用 "default"）
c2rust-translate translate

# 自动处理所有文件，不进行交互提示
c2rust-translate translate --feature <特性名称> --allow-all

# 自定义最大修复尝试次数（默认为 10）
c2rust-translate translate --feature <特性名称> --max-fix-attempts 5

# 显示完整的代码和错误输出（不截断）
c2rust-translate translate --feature <特性名称> --show-full-output

# 组合多个选项使用
c2rust-translate translate --feature <特性名称> --allow-all --show-full-output
```

### 文件选择模式

工具提供两种文件处理模式：

#### 1. 交互模式（默认）

当不使用 `--allow-all` 选项时，工具会：
- 列出所有未处理的 `.rs` 文件（按字母顺序排序）
- 显示文件序号和相对路径
- 提示用户选择要处理的文件

支持的选择格式：
- **单个文件**: `1` 或 `3`
- **多个文件**: `1,3,5`（逗号分隔）
- **范围**: `1-3`（处理文件 1、2、3）
- **混合**: `1,3-5,7`（处理文件 1、3、4、5、7）
- **所有文件**: `all` 或 `ALL`（大小写不敏感）

**示例输出：**
```
Available files to process:
  1. fun_alpha.rs
  2. fun_beta.rs
  3. var_gamma.rs
  4. var_zeta.rs

Select files to process:
  - Enter numbers separated by commas (e.g., 1,3,5)
  - Enter ranges (e.g., 1-3,5)
  - Enter 'all' to process all files

Your selection: 1,3-4
```

#### 2. 自动处理模式 (`--allow-all`)

使用 `--allow-all` 选项时：
- 不显示文件列表
- 不等待用户输入
- 自动处理所有未处理的文件
- 适合自动化脚本和批处理场景

### 翻译工作流程

该命令将执行以下操作：

1. **初始化**（如需要）- 检查并初始化 `<特性名称>/rust` 目录
2. **目标制品选择** - 提示用户从 `.c2rust/<特性名称>/c/targets.list` 中选择目标制品
   - 自动读取并去重制品列表
   - 如果只有一个制品，自动选择
   - 选择结果存储到配置文件中（`build.target`）
3. **文件翻译** - 对每个空的 `.rs` 文件：
   - 根据文件名前缀（`var_` 或 `fun_`）确定类型
   - 翻译对应的 `.c` 文件为 Rust
   - 构建并自动修复编译错误（默认最多 10 次尝试，可通过 `--max-fix-attempts` 选项配置）
   - Git 提交更改
   - 更新代码分析
   - 运行混合构建测试

#### 文件命名规范
- `var_*.rs` - 变量声明
- `fun_*.rs` - 函数定义

每个 `.rs` 文件应有对应的同名 `.c` 文件。

#### 必需工具

**核心依赖：**
- `code-analyse` - 代码分析和初始化
- `translate_and_fix.py` - C 到 Rust 翻译和错误修复
- `c2rust-config` - 配置管理（需配置 build/test/clean 的 dir 和 cmd）

#### 项目结构

```
项目根目录/
├── .c2rust/
│   ├── config.toml           # 配置文件
│   └── <feature>/
│       ├── c/
│       │   └── targets.list  # 目标制品列表
│       └── rust/             # 待翻译的 .rs 和 .c 文件
│           ├── Cargo.toml    # Rust 项目配置
│           ├── fun_*.rs / .c # 函数翻译文件
│           └── var_*.rs / .c # 变量翻译文件
```

#### 工具用法示例

**翻译工具调用格式：**
```bash
# 变量翻译
python translate_and_fix.py --config <config.toml> --type var --c_code <input.c> --output <output.rs>

# 函数翻译
python translate_and_fix.py --config <config.toml> --type fn --c_code <input.c> --output <output.rs>

# 语法修复（无用户建议）
python translate_and_fix.py --config <config.toml> --type syntax_fix --c_code <c_code.c> --rust_code <rust_code.rs> --output <output.rs> --error <error.txt>

# 语法修复（有用户建议）
python translate_and_fix.py --config <config.toml> --type syntax_fix --c_code <c_code.c> --rust_code <rust_code.rs> --output <output.rs> --error <error.txt> --suggestion <suggestion.txt>
```

**代码分析工具：**
```bash
code-analyse --init --feature <特性名称>     # 初始化
code-analyse --update --feature <特性名称>   # 更新
```

## 示例

在项目根目录（包含 `.c2rust/` 目录）或其子目录中运行：

```bash
# 翻译名为 "my_feature" 的特性
c2rust-translate translate --feature my_feature

# 翻译默认特性（使用 "default" 作为特性名称）
c2rust-translate translate
```

这将处理 `<特性名称>/rust/` 目录中所有空的 `.rs` 文件。

## 错误处理

工具在遇到编译错误或测试失败时提供智能的交互式错误处理：

### 自动修复尝试

- 编译错误会自动尝试修复（默认最多 10 次，可通过 `--max-fix-attempts` 配置）
- 每次修复失败后，自动应用下一次修复

### 交互式错误处理

当达到最大修复尝试次数后，工具会提供三个选项供用户选择：

#### 1. 继续尝试（Continue）
- 允许用户输入修复建议提示词
- 提示词会保存到项目根目录的 `c2rust.md` 文件中
- 后续的修复尝试会使用这些提示词作为参考
- **对于测试失败**：必须输入修复建议才能继续

**使用场景**：
- 您已经分析了错误原因，想要给 AI 提供更具体的修复方向
- 需要添加特定的约束或要求到修复过程中

#### 2. 手工修复（Manual Fix）
- 自动在 vim 中打开失败的 Rust 文件
- 完整显示 C 源代码和 Rust 代码（不截断）
- 手工修改后会自动重新执行构建
  - 对于构建失败：手工修改保存后会自动重新执行构建
  - 对于测试失败：手工修改保存后会自动重新执行构建和测试
- 如果修改后仍有错误，可以继续选择处理方式

**使用场景**：
- 错误原因明确，手工修改更快
- 需要进行复杂的代码重构
- AI 修复多次失败，需要人工介入

#### 3. 退出（Exit）
- 停止翻译过程并退出工具
- 未完成的文件不会被处理

**使用场景**：
- 遇到无法解决的问题，需要停止处理
- 当前任务无法继续，需要修改项目配置或依赖后重新运行

### 适用场景

交互式错误处理适用于以下情况：

1. **编译失败**：达到最大修复尝试次数后
2. **测试失败**：混合构建测试失败时
3. **启动时失败**：翻译开始前的初始构建或测试失败

### 建议文件（c2rust.md）

- **位置**：项目根目录下的 `c2rust.md`
- **格式**：Markdown 格式，带时间戳
- **用途**：存储用户输入的修复建议，供 translate_and_fix.py 参考
- **持久化**：建议会累积保存，可以在多次翻译过程中复用

### 示例流程

```
⚠ Maximum fix attempts reached!
File example.rs still has build errors after 10 fix attempts.

File Locations:
  C file:    /path/to/example.c
  Rust file: /path/to/example.rs

═══ C Source Code (Full) ═══
[显示完整C代码]

═══ Rust Code (Full) ═══
[显示完整Rust代码]

═══ Build Error ═══
[显示编译错误]

⚠ Build failure - What would you like to do?

Available options:
  1. Continue trying (optionally enter a fix suggestion)
  2. Manual fix (edit the file directly)
  3. Exit (abort the translation process)

Enter your choice (1/2/3): 1

Please enter your fix suggestion:
(The suggestion will be saved and used in the next fix attempt)
(Press Enter to skip entering a suggestion)

Suggestion: Please use std::ffi::CStr instead of raw pointer manipulation

✓ Suggestion recorded: Please use std::ffi::CStr instead of raw pointer manipulation
✓ Suggestion saved to /path/to/project/c2rust.md
```

### 错误类型

工具会在以下情况下提供交互选项或停止执行：

**提供交互选项的情况**：
- 构建错误修复超过配置的最大尝试次数（默认 10 次）- 提供继续/手工修复/退出选项
- 混合构建测试失败 - 提供继续/手工修复/退出选项（继续需要输入建议）
- 启动时的初始构建失败 - 提供继续/手工修复/退出选项
- 启动时的初始测试失败 - 提供继续/手工修复/退出选项

**直接停止的情况**：
- Rust 目录初始化失败
- 缺少对应的 `.c` 文件
- 翻译失败
- Git 操作失败（add/commit，"nothing to commit" 除外）
- 用户选择"退出"选项

## Git 集成

工具会自动提交更改：
- 初始化后：`"Initialize <feature> rust directory"`
- 翻译成功后：`"Translate <filename> from C to Rust (feature: <feature>)"`
- 更新分析后：`"Update code analysis for <feature>"`

## 代码结构

项目采用模块化设计，主要模块包括：

- `analyzer` - 代码分析功能（初始化和更新）
- `builder` - 构建相关功能（cargo 构建、混合构建）
- `file_scanner` - 文件扫描和类型提取
- `git` - Git 版本控制操作
- `translator` - C 到 Rust 的翻译和错误修复
- `lib` - 主要翻译工作流程编排

## 许可证

本项目当前尚未正式选择开源许可证。

在许可证明确之前，除非另有书面授权，您不应将本项目用于再分发或商业用途。如需商业使用，请联系项目维护者。

## 贡献

欢迎通过 Issue 和 Pull Request 贡献代码或提出建议。提交贡献时请尽量：

- 在 Issue 中清晰描述问题或需求
- 对于代码修改，先在本地通过相关构建与测试
- 在 Pull Request 中说明变更目的、主要修改点以及可能的影响范围
- 确保所有测试通过且代码没有警告
- 遵循现有的代码风格和模块结构

### 提交 Pull Request 的步骤：

1. Fork 本仓库
2. 创建您的特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交您的更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 打开一个 Pull Request

我们会尽快审查您的贡献。