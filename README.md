# c2rust-translate

## 使用说明

### 项目结构
```
<项目根目录>/
├── .c2rust/
│   └── config.toml       # 大模型配置文件
└── tools/
    └── translate_and_fix/
        ├── translate_and_fix.py
        └── ...
```

### 配置文件
在项目根目录下创建 `.c2rust/config.toml` 文件，配置大模型相关信息。

示例配置：
```toml
[model]
model = "glm-4.6"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key = "your-api-key-here"
temperature = 0
top_p = 1
seed = 42
max_retries = 10
timeout = 500
```

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