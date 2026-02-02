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

以下工具必须在你的 PATH 中可用：

- `code-analyse` - 用于代码分析和初始化
- `translate_and_fix.py` - 用于翻译和错误修复的 Python 脚本
- `c2rust-config` - 用于配置管理（可选）
- `c2rust-clean`、`c2rust-build`、`c2rust-test` - 用于混合构建测试（可选）

#### 环境变量

需要设置以下环境变量：

- `C2RUST_PROJECT_ROOT` - 项目根目录，用于定位配置文件 `.c2rust/config.toml`

#### 翻译工具用法

该工具使用以下参数调用 `translate_and_fix.py`：

```bash
# 用于翻译
python translate_and_fix.py --config $C2RUST_PROJECT_ROOT/.c2rust/config.toml --type <var|fn> --code <c文件> --output <rs文件>

# 用于修复错误
python translate_and_fix.py --config $C2RUST_PROJECT_ROOT/.c2rust/config.toml --type <var|fn> --error <错误文件> --output <rs文件>
```

#### 代码分析工具用法

```bash
# 初始化
code-analyse --init --feature <特性名称>

# 更新
code-analyse --update --feature <特性名称>
```

#### 混合构建配置工具用法

`c2rust-config` 用于获取构建、测试和清理命令：

```bash
# 获取构建命令
c2rust-config config --make --feature <特性名称> --list build

# 获取测试命令
c2rust-config config --make --feature <特性名称> --list test

# 获取清理命令
c2rust-config config --make --feature <特性名称> --list clean
```

## 示例

```bash
# 翻译名为 "my_feature" 的特性
c2rust-translate translate --feature my_feature

# 翻译默认特性（使用 "default" 作为特性名称）
c2rust-translate translate
```

这将处理 `<特性名称>/rust/` 目录中所有空的 `.rs` 文件。

## 错误处理

- 如果 rust 目录初始化失败，工具将退出并显示错误
- 如果缺少对应的 `.c` 文件，工具将发出警告并跳过该文件
- 如果翻译或错误修复失败，工具将退出并显示错误
- 如果混合构建测试失败，工具将发出警告但继续执行

## Git 集成

工具会在以下时间点自动提交更改：

1. 初始化 rust 目录后
2. 成功翻译每个文件后
3. 更新代码分析后

提交消息格式如下：
- `"Initialize <feature> rust directory"` （初始化 <特性> rust 目录）
- `"Translate <feature> from C to Rust"` （将 <特性> 从 C 翻译为 Rust）
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

[在此添加许可证]

## 贡献

[在此添加贡献指南]