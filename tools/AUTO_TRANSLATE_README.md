# 自动化C到Rust翻译工作流工具

## 概述

`auto_translate.py` 是一个自动化工具，用于管理C到Rust的翻译流程，包括初始化、翻译、编译修复和代码分析更新。

## 功能特性

- **自动初始化检查**：检查并初始化 `<feature>/rust` 目录
- **智能文件扫描**：自动扫描并识别空的 `.rs` 文件
- **类型识别**：根据文件名前缀（`var_` 或 `fun_`）自动识别变量或函数类型
- **自动翻译**：调用 `translate_and_fix.py` 进行C到Rust的翻译
- **编译错误修复**：自动检测编译错误并尝试修复
- **版本控制集成**：在关键步骤自动执行 git commit
- **代码分析更新**：执行 `code-analyse --update` 保持代码分析最新
- **混合构建支持**：支持混合构建和测试流程
- **用户交互**：在关键决策点与用户交互

## 依赖工具

在使用本工具之前，请确保以下工具已安装并可用：

1. **c2rust-translate** - C到Rust翻译工具
2. **code-analyse** - 代码分析工具
3. **cargo** - Rust包管理器和构建工具
4. **git** - 版本控制系统
5. **c2rust-config** (可选) - 配置管理工具
6. **c2rust-clean/build/test** (可选) - 构建和测试工具
7. **Python 3.x** - Python运行环境

## 安装

无需特殊安装，直接使用即可：

```bash
chmod +x tools/auto_translate.py
```

## 使用方法

### 基本用法

```bash
python3 tools/auto_translate.py --feature <feature>
```

### 指定项目根目录

```bash
python3 tools/auto_translate.py --feature <feature> --project-root /path/to/project
```

### 命令行参数

- `--feature FEATURE`（必需）：功能名称，对应项目中的 feature 目录
- `--project-root PROJECT_ROOT`（可选）：项目根目录路径，默认为当前工作目录

### 示例

```bash
# 在当前目录处理名为 'myfeature' 的功能
python3 tools/auto_translate.py --feature myfeature

# 指定项目根目录
python3 tools/auto_translate.py --feature network --project-root /home/user/c2rust-project
```

## 工作流程

### 1. 初始化检查

工具启动后首先检查 `<feature>/rust` 目录是否存在：

- 如果目录存在，继续下一步
- 如果目录不存在：
  - 执行 `code-analyse --feature <feature> --init`
  - 检查初始化结果
  - 成功后执行 `git commit` 保存变更

### 2. 主循环处理

循环处理所有空的 `.rs` 文件，直到没有空文件为止：

#### 2.1 扫描空文件

扫描 `<feature>/rust` 目录下所有空的 `.rs` 文件

#### 2.2 处理每个空文件

对于每个空文件：

1. **提取类型**：根据文件名前缀识别类型
   - `var_` → 变量类型
   - `fun_` → 函数类型

2. **检查C文件**：查找对应的 `.c` 文件
   - 如果不存在，询问用户是否重新初始化

3. **翻译**：调用 `translate_and_fix.py` 进行翻译
   ```bash
   python translate_and_fix.py --config config.toml --type <type> --code <c_file> --output <rs_file>
   ```

4. **编译与修复循环**（最多5次）：
   - 执行 `cargo build`
   - 如果有编译错误且与当前文件相关：
     - 提取错误信息
     - 调用 `translate_and_fix.py` 进行修复
     - 重复编译
   - 如果编译成功或错误与其他文件相关，继续

5. **提交翻译**：`git commit -m "Translate <filename>"`

6. **更新分析**：执行 `code-analyse --feature <feature> --update`

7. **提交更新**：`git commit -m "Update code analysis for <filename>"`

8. **混合构建**：
   - 获取并执行 clean/build/test 命令
   - 设置环境变量：
     - `LD_PRELOAD=<混合构建库>`（如果提供）
     - `C2RUST_FEATURE_ROOT=<feature目录>`

### 3. 完成

当所有空文件处理完成后，工具退出。

## 环境变量

- `C2RUST_HYBRID_LIB`（可选）：混合构建库路径，用于设置 `LD_PRELOAD`

## 错误处理

工具在以下情况会退出并报告错误：

- 初始化失败
- 找不到对应的C文件且用户选择不重新初始化
- 翻译失败
- 翻译成功但输出为空
- 修复失败
- 达到最大修复尝试次数（5次）仍有错误
- `code-analyse --update` 失败
- 混合构建失败

## 日志输出

工具提供详细的日志输出，包括：

- 执行的命令
- 处理的文件
- 编译状态
- 错误信息
- 进度信息

日志格式：
```
2024-01-01 12:00:00 - INFO - 消息内容
```

## 项目结构要求

工具期望的项目结构：

```
project_root/
├── .c2rust/
│   └── config.toml
├── <feature>/
│   ├── rust/
│   │   ├── var_something.rs
│   │   ├── fun_something.rs
│   │   └── ...
│   ├── var_something.c
│   ├── fun_something.c
│   └── ...
└── tools/
    ├── auto_translate.py
    └── translate_and_fix/
        ├── translate_and_fix.py
        ├── config.toml
        └── ...
```

## 配置文件

工具使用以下配置文件：

1. **`.c2rust/config.toml`**：项目配置文件
2. **`tools/translate_and_fix/config.toml`**：翻译工具配置文件

## 常见问题

### Q: 工具提示找不到C文件怎么办？

A: 检查以下几点：
- C文件是否存在于正确的位置
- 文件名是否与Rust文件对应（除了后缀）
- 尝试重新执行 `code-analyse --init`

### Q: 编译错误一直无法修复怎么办？

A: 
- 检查错误信息
- 可能需要手动修复某些复杂的错误
- 检查配置文件是否正确
- 查看大模型API是否可用

### Q: 如何停止工具？

A: 使用 `Ctrl+C` 中断执行

### Q: 工具会修改哪些文件？

A: 
- 翻译的Rust文件
- Git提交历史
- 不会修改C源文件

## 技术细节

### 文件类型识别

工具通过文件名前缀识别类型：
- `var_*.rs` → 变量翻译（`--type var`）
- `fun_*.rs` → 函数翻译（`--type fn`）

### 编译错误检测

工具检查错误信息中是否包含当前处理的文件名，以确定错误是否与该文件相关。

### 修复策略

- 最多尝试修复5次
- 每次修复后重新编译
- 如果错误与其他文件相关，跳过修复

### Git集成

工具在以下时机执行提交：
1. 初始化成功后
2. 每个文件翻译成功后
3. 代码分析更新后

## 开发和调试

### 启用详细日志

修改脚本中的日志级别：

```python
logging.basicConfig(
    level=logging.DEBUG,  # 改为DEBUG
    ...
)
```

### 跳过Git提交

临时禁用Git提交功能（用于测试）：

```python
def git_commit(self, message: str) -> bool:
    logger.info(f"[SKIP] Git commit: {message}")
    return True
```

## 贡献

欢迎提交问题报告和改进建议。

## 许可证

与项目主仓库保持一致。

## 版本历史

- **v1.0.0** (2024) - 初始版本
  - 实现完整的自动翻译工作流
  - 支持变量和函数翻译
  - 集成编译错误修复
  - 混合构建支持
