# 使用示例

## 示例 1: 基本使用

假设你有一个名为 `network` 的功能需要从C翻译到Rust：

```bash
# 进入项目根目录
cd /path/to/c2rust-project

# 运行自动翻译工具
python3 tools/auto_translate.py --feature network
```

**预期输出：**

```
============================================================
开始自动化C到Rust翻译工作流
Feature: network
项目根目录: /path/to/c2rust-project
============================================================
2024-01-01 10:00:00 - INFO - 检查 /path/to/c2rust-project/network/rust 目录是否存在
2024-01-01 10:00:01 - INFO - 目录 /path/to/c2rust-project/network/rust 已存在
2024-01-01 10:00:02 - INFO - 发现 3 个空文件待处理
2024-01-01 10:00:03 - INFO - 处理空文件: /path/to/c2rust-project/network/rust/var_socket.rs
2024-01-01 10:00:04 - INFO - 开始翻译 var_socket.c -> var_socket.rs (类型: var)
...
```

## 示例 2: 指定项目根目录

如果你不在项目根目录下：

```bash
python3 /path/to/tools/auto_translate.py \
    --feature database \
    --project-root /path/to/c2rust-project
```

## 示例 3: 带环境变量的使用

如果需要使用混合构建库：

```bash
# 设置环境变量
export C2RUST_HYBRID_LIB=/path/to/hybrid_build.so

# 运行工具
python3 tools/auto_translate.py --feature utils
```

## 示例 4: 处理初始化场景

第一次使用时，如果 `rust` 目录不存在：

```bash
python3 tools/auto_translate.py --feature newfeature
```

**预期输出：**

```
2024-01-01 10:00:00 - INFO - 检查 .../newfeature/rust 目录是否存在
2024-01-01 10:00:01 - INFO - 目录 .../newfeature/rust 不存在，执行初始化
2024-01-01 10:00:02 - INFO - 执行命令: code-analyse --feature newfeature --init
2024-01-01 10:00:10 - INFO - 提交初始化的文件
2024-01-01 10:00:11 - INFO - 已提交变更: Initialize newfeature directory with code-analyse
...
```

## 示例 5: 处理项目被破坏的情况

如果在处理过程中发现C文件缺失：

```bash
python3 tools/auto_translate.py --feature myfeature
```

**交互输出：**

```
2024-01-01 10:00:00 - WARNING - 找不到对应的C文件: .../myfeature/rust/var_test.c
2024-01-01 10:00:01 - ERROR - 找不到对应的C文件
工程可能被破坏，是否需要执行 code-analyse --init? (y/n): y
2024-01-01 10:00:05 - INFO - 目录 .../myfeature/rust 不存在，执行初始化
...
```

## 示例 6: 监控工作流程

完整的工作流程示例输出：

```bash
python3 tools/auto_translate.py --feature example
```

```
============================================================
开始自动化C到Rust翻译工作流
Feature: example
项目根目录: /home/user/project
============================================================
2024-01-01 10:00:00 - INFO - 检查 /home/user/project/example/rust 目录是否存在
2024-01-01 10:00:00 - INFO - 目录 /home/user/project/example/rust 已存在
2024-01-01 10:00:01 - INFO - 发现空文件: /home/user/project/example/rust/var_config.rs
2024-01-01 10:00:01 - INFO - 发现 1 个空文件待处理
2024-01-01 10:00:02 - INFO - 执行 cargo build
2024-01-01 10:00:05 - WARNING - cargo build 失败
2024-01-01 10:00:05 - INFO - 处理空文件: /home/user/project/example/rust/var_config.rs
2024-01-01 10:00:06 - INFO - 开始翻译 var_config.c -> var_config.rs (类型: var)
2024-01-01 10:00:07 - INFO - 执行命令: python3 .../translate_and_fix.py ...
2024-01-01 10:00:15 - INFO - 翻译/修复结果已写入 var_config.rs
2024-01-01 10:00:15 - INFO - 翻译成功: var_config.rs
2024-01-01 10:00:16 - INFO - 执行 cargo build
2024-01-01 10:00:20 - INFO - cargo build 成功
2024-01-01 10:00:20 - INFO - 编译成功
2024-01-01 10:00:21 - INFO - 执行命令: git add .
2024-01-01 10:00:21 - INFO - 执行命令: git commit -m Translate var_config.rs
2024-01-01 10:00:22 - INFO - 已提交变更: Translate var_config.rs
2024-01-01 10:00:22 - INFO - 执行 code-analyse --update
2024-01-01 10:00:25 - INFO - code-analyse --update 执行成功
2024-01-01 10:00:25 - INFO - 已提交变更: Update code analysis for var_config.rs
2024-01-01 10:00:26 - INFO - 执行clean命令: make clean
2024-01-01 10:00:27 - INFO - 执行build命令: make
2024-01-01 10:00:30 - INFO - 执行test命令: make test
2024-01-01 10:00:35 - INFO - 所有构建命令执行成功
2024-01-01 10:00:35 - INFO - 文件处理完成: var_config.rs
2024-01-01 10:00:36 - INFO - 没有发现空的rs文件，工作完成
============================================================
自动化翻译工作流完成！
============================================================
```

## 错误场景示例

### 场景 1: 翻译工具失败

```
2024-01-01 10:00:00 - ERROR - 翻译失败: ERROR - 加载配置文件/创建大模型会话出错
2024-01-01 10:00:00 - ERROR - 翻译失败: var_test.rs
2024-01-01 10:00:00 - ERROR - 处理文件失败: .../var_test.rs，退出
```

### 场景 2: 修复失败

```
2024-01-01 10:00:00 - INFO - 尝试修复编译错误 (第 1/5 次)
2024-01-01 10:00:05 - ERROR - 修复失败: ...
2024-01-01 10:00:05 - ERROR - 修复失败
```

### 场景 3: 达到最大修复次数

```
2024-01-01 10:00:00 - INFO - 尝试修复编译错误 (第 5/5 次)
2024-01-01 10:00:05 - ERROR - 达到最大修复尝试次数 (5)，仍有编译错误
```

## 技巧和最佳实践

### 1. 在开始前检查依赖

```bash
# 检查所有必需的工具是否可用
which python3 cargo git code-analyse c2rust-config

# 检查Python版本
python3 --version  # 应该是 3.6 或更高
```

### 2. 准备配置文件

确保以下配置文件存在并正确配置：
- `.c2rust/config.toml` - 项目配置
- `tools/translate_and_fix/config.toml` - 翻译工具配置

### 3. 使用日志重定向

```bash
# 保存日志到文件
python3 tools/auto_translate.py --feature myfeature 2>&1 | tee translation.log
```

### 4. 在后台运行长时间任务

```bash
# 使用 nohup 在后台运行
nohup python3 tools/auto_translate.py --feature large_module > translation.log 2>&1 &

# 查看进度
tail -f translation.log
```

### 5. 分批处理

如果有大量文件，可以考虑先处理一部分：

```bash
# 先处理几个文件，检查是否正常
# 可以手动删除一些空文件，只保留要测试的文件

# 运行工具
python3 tools/auto_translate.py --feature test

# 检查结果后，恢复其他文件并继续
```

### 6. 监控Git历史

```bash
# 查看工具创建的提交
git log --oneline --author="auto_translate"

# 如果需要回滚
git revert <commit_hash>
```

## 故障排除

### 问题：工具一直卡在某个步骤

**解决方案：**
- 检查网络连接（LLM API调用需要网络）
- 检查API密钥是否有效
- 使用 Ctrl+C 中断并查看日志

### 问题：编译错误无法修复

**解决方案：**
- 手动检查生成的Rust代码
- 查看错误信息是否与当前文件相关
- 可能需要手动修复复杂的错误

### 问题：Git提交失败

**解决方案：**
- 检查是否有未解决的合并冲突
- 确保有正确的Git配置（user.name, user.email）
- 检查文件权限

## 集成到CI/CD

可以将工具集成到自动化流程中：

```yaml
# GitHub Actions 示例
name: Auto Translate
on: [push]
jobs:
  translate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Setup Python
        uses: actions/setup-python@v2
        with:
          python-version: '3.x'
      - name: Run auto translate
        run: |
          python3 tools/auto_translate.py --feature ${{ matrix.feature }}
        env:
          C2RUST_HYBRID_LIB: /path/to/lib.so
```
