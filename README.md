# c2rust-translate

C到Rust翻译工具集

## 工具列表

### 1. auto_translate.py - 自动化翻译工作流工具

自动化管理C到Rust的完整翻译流程，包括初始化、翻译、编译修复、代码分析更新和混合构建。

**位置**: `tools/auto_translate.py`

**使用方法**:
```bash
python3 tools/auto_translate.py --feature <feature>
```

**详细文档**: 参见 [tools/AUTO_TRANSLATE_README.md](tools/AUTO_TRANSLATE_README.md)

### 2. translate_and_fix.py - 翻译和修复工具

底层翻译工具，用于单个C文件到Rust的翻译和语法修复。

**位置**: `tools/translate_and_fix/translate_and_fix.py`

**使用方法**:
```bash
# 变量翻译
python translate_and_fix.py --config config.toml --type var --code code.c --output output.rs

# 函数翻译
python translate_and_fix.py --config config.toml --type fn --code code.c --output output.rs

# 语法修复
python translate_and_fix.py --config config.toml --type fix --code code.rs --output output.rs --error error.txt
```

**详细文档**: 参见 `tools/translate_and_fix/说明文档.md`

## 快速开始

1. 准备项目环境和依赖工具
2. 使用 `auto_translate.py` 自动处理整个翻译流程：
   ```bash
   python3 tools/auto_translate.py --feature myfeature
   ```

## 项目结构

```
c2rust-translate/
├── README.md
└── tools/
    ├── auto_translate.py              # 自动化翻译工作流工具
    ├── AUTO_TRANSLATE_README.md       # 详细使用文档
    ├── test_auto_translate.py         # 单元测试
    └── translate_and_fix/             # 翻译和修复工具目录
        ├── translate_and_fix.py       # 主工具脚本
        ├── config.toml                # 配置文件
        ├── LLM_calling.py             # LLM调用
        ├── function_translation.py    # 函数翻译
        ├── variable_translation.py    # 变量翻译
        ├── syntax_fixing.py           # 语法修复
        └── 说明文档.md                # 详细文档
```

## 依赖工具

- Python 3.x
- c2rust-translate
- code-analyse
- cargo
- git
- c2rust-config (可选)
- c2rust-clean/build/test (可选)

## 贡献

欢迎提交问题报告和改进建议。

## 许可证

待定