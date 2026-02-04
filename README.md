# c2rust-translate

一个使用 c2rust 框架自动化 C 代码到 Rust 代码翻译的工具。

## 功能特性

- 自动化的 C 到 Rust 翻译工作流
- 支持基于特性(feature)的翻译，使用 `--feature` 标志
- 自动初始化 Rust 项目结构
- 集成翻译工具（`translate_and_fix.py`）
- 自动检测和修复构建错误
- 基于 Git 的版本控制集成
- 代码分析集成（`code-analyse`）
- 混合构建测试支持

## 安装

从源码构建：

```bash
cargo build --release
```

二进制文件将位于 `target/release/c2rust-translate`。

## 使用方法

### 翻译一个特性

```bash
# 翻译指定的特性
c2rust-translate translate --feature <特性名称>

# 翻译默认特性（不指定 --feature 时默认使用 "default"）
c2rust-translate translate
```

该命令将执行以下操作：

1. **初始化** - 检查 `<特性名称>/rust` 目录是否存在，如果需要则进行初始化
2. **扫描** - 查找 rust 目录中所有空的 `.rs` 文件
3. **翻译** - 对每个空的 `.rs` 文件：
   - 根据文件名前缀确定类型（变量或函数）（`var_` 或 `fun_`）
   - 将对应的 `.c` 文件翻译为 Rust
   - 构建项目并修复任何编译错误
   - 使用 git 提交更改
   - 更新代码分析
   - 运行混合构建测试

### 工作流程详情

#### 文件命名规范

- `var_*.rs` - 变量声明
- `fun_*.rs` - 函数定义

每个 `.rs` 文件应该有一个同名的对应 `.c` 文件。

#### 必需工具

本工具依赖一些外部命令行工具，其中一部分是必需的，另一部分是可选的。

**必需依赖（必须在你的 PATH 中可用）：**

- `code-analyse` - 用于代码分析和初始化
- `translate_and_fix.py` - 用于翻译和错误修复的 Python 脚本

**可选依赖（存在于 PATH 中时将启用额外功能）：**

- `c2rust-config` - 用于配置管理（混合构建需要）

**注意：** 混合构建不再使用 `c2rust-{build,test,clean}` 包装命令。相反，它直接从 `c2rust-config` 获取构建目录和命令，然后在项目目录中执行原生的构建命令（如 make、cmake 等）。对于构建操作，会保留 LD_PRELOAD 机制以拦截系统调用。

如果 `c2rust-config` 不可用或配置缺失，混合构建测试将失败并停止翻译流程。确保 `c2rust-config` 已正确安装并配置了以下所有必需的配置键：
- `build.dir` 和 `build.cmd` - 构建操作的目录和命令
- `test.dir` 和 `test.cmd` - 测试操作的目录和命令
- `clean.dir` 和 `clean.cmd` - 清理操作的目录和命令

#### 项目结构要求

工具会自动向上搜索 `.c2rust` 目录来定位项目根目录。项目应具有以下结构：

```
项目根目录/
├── .c2rust/
│   └── config.toml    # 配置文件
└── <feature>/
    └── rust/          # 包含待翻译的 .rs 和 .c 文件
                       # 每个特性都有自己的 Cargo.toml（cargo build 在此执行）
```

#### 翻译工具用法

该工具使用以下参数调用 `translate_and_fix.py`：

```bash
# 用于翻译（配置文件路径自动从 .c2rust 目录定位）
python translate_and_fix.py --config <项目根>/.c2rust/config.toml --type <var|fn> --code <c文件> --output <rs文件>

# 用于修复错误
python translate_and_fix.py --config <项目根>/.c2rust/config.toml --type <var|fn> --error <错误文件> --output <rs文件>
```

#### 代码分析工具用法

```bash
# 初始化
code-analyse --init --feature <特性名称>

# 更新
code-analyse --update --feature <特性名称>
```

#### 混合构建配置工具用法

`c2rust-config` 用于获取构建目录和命令配置。所有配置键都是必需的，并且是特性特定的（通过 `--feature` 参数指定）：

```bash
# 获取构建目录（必需）
c2rust-config config --make --feature <特性名称> --list build.dir

# 获取构建命令（必需）
c2rust-config config --make --feature <特性名称> --list build.cmd

# 获取测试目录（必需）
c2rust-config config --make --feature <特性名称> --list test.dir

# 获取测试命令（必需）
c2rust-config config --make --feature <特性名称> --list test.cmd

# 获取清理目录（必需）
c2rust-config config --make --feature <特性名称> --list clean.dir

# 获取清理命令（必需）
c2rust-config config --make --feature <特性名称> --list clean.cmd
```

这些配置用于在正确的目录下执行原生构建命令（make、cmake 等），而不是通过 `c2rust-{build,test,clean}` 包装命令。每个操作（build、test、clean）在各自配置的目录中执行各自的命令。配置是特性特定的，允许不同特性使用不同的构建配置。对于构建操作，会自动应用 LD_PRELOAD 机制以拦截系统调用。

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

- 如果 rust 目录初始化失败，工具将退出并显示错误
- 如果缺少对应的 `.c` 文件，工具将退出并显示错误
- 如果翻译或错误修复失败，工具将退出并显示错误
- 如果构建错误修复超过 5 次尝试后仍失败，工具将退出并显示错误
- 如果混合构建测试失败，工具将退出并显示错误
- 如果 git add 操作失败，工具将退出并显示错误
- 如果 git commit 操作失败（除了"nothing to commit"情况），工具将退出并显示错误
- 如果最终构建失败，即使所有文件已翻译完成，工具也会退出并显示错误
- 如果未安装 c2rust-config，混合构建测试将被跳过（不会报错）

## Git 集成

工具会在以下时间点自动提交更改：

1. 初始化 rust 目录后
2. 成功翻译每个文件后
3. 更新代码分析后

提交消息格式如下：
- `"Initialize <feature> rust directory"` （初始化 <特性> rust 目录）
- `"Translate <filename> from C to Rust (feature: <feature>)"` （将 <文件名> 从 C 翻译为 Rust，特性：<特性>）
- `"Update code analysis for <feature>"` （更新 <特性> 的代码分析）

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