# c2rust-translate

## 使用说明

### 项目结构
```
<项目根目录>/
├── .c2rust/
│   ├── config.toml.example  # 配置文件模板
│   └── config.toml          # 实际配置（不提交到 git，包含 API key）
└── tools/
    └── translate_and_fix/
        ├── translate_and_fix.py
        └── ...
```

### 配置文件
在项目根目录下创建 `.c2rust/config.toml` 文件，配置大模型相关信息。

**首次设置：**
```bash
# 复制示例配置文件
cp .c2rust/config.toml.example .c2rust/config.toml

# 编辑配置文件，填入你的 API key
# 编辑 .c2rust/config.toml 中的 api_key 字段
```

示例配置：
```toml
[model]
model = "glm-4.6"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key = "your-api-key-here"  # 请替换为你的实际 API key
temperature = 0
top_p = 1
seed = 42
max_retries = 10
timeout = 500
```

**注意：** `.c2rust/config.toml` 包含敏感信息（API key），已被 `.gitignore` 排除，不会被提交到版本控制。

### 使用方法

#### 变量翻译
```bash
python tools/translate_and_fix/translate_and_fix.py --type var --code code.c --output output.rs
```

#### 函数翻译
```bash
python tools/translate_and_fix/translate_and_fix.py --type fn --code code.c --output output.rs
```

#### 语法修复
```bash
python tools/translate_and_fix/translate_and_fix.py --type fix --code code.rs --output output.rs --error error.txt
```

### 注意事项
- 配置文件必须位于 `<C2RUST_PROJECT_ROOT>/.c2rust/config.toml`
- 工具会自动从该位置读取配置，无需通过命令行参数指定
- 详细文档请参见 `tools/translate_and_fix/说明文档.md`