import argparse
import os
import sys

from LLM_calling import load_LLM_config, init_openai_instance, close_openai_instance
from function_translation import function_translation
from variable_translation import var_translation
from syntax_fixing import syntax_fixing

# 输入参数。error参数只在调用语法修复时需要填写；其他所有参数每次调用时都必须填写
parser = argparse.ArgumentParser(description='Translate and fix')
parser.add_argument('--type', type=str, help='Type of translate tools. Allowed values: "var", "fn", "fix"')
parser.add_argument('--code', type=str, help='The input file path. Input C code for var/fun translation, or input rust code for syntax fixing. The input file should ONLY contain the code that are about to be translated.')
parser.add_argument('--output', type=str, help='The output file path of LLM translation/fixing result. The content of output file will be OVEWRITTEN.')
parser.add_argument('--error', type=str, default=None, help='The file path of error message for syntax fixing. The error message file should ONLY contain the error message about the code in the input file.')
args = parser.parse_args()

# 自动查找配置文件路径
# tools目录的父目录为项目根目录，配置文件位于 <C2RUST_PROJECT_ROOT>/.c2rust/config.toml
script_dir = os.path.dirname(os.path.abspath(__file__))  # tools/translate_and_fix
tools_dir = os.path.dirname(script_dir)  # tools
project_root = os.path.dirname(tools_dir)  # 项目根目录
config_path = os.path.join(project_root, '.c2rust', 'config.toml')

# 检查配置文件是否存在
if not os.path.exists(config_path):
    print(f"ERROR - 配置文件不存在: {config_path}")
    print(f"请确保在项目根目录下创建 .c2rust/config.toml 配置文件")
    sys.exit(1)

# 根据config字段创建大模型会话
try:
    LLM_config = load_LLM_config(config_path)
    openai_instance = init_openai_instance(LLM_config)
except Exception as e:
    print(f"ERROR - 加载配置文件/创建大模型会话出错: {e}")
    exit(1)

# 加载待翻译/修复的代码
try:
    with open(args.code, 'r', encoding='utf-8') as file:
        code = file.read()
except Exception as e:
    # 关闭大模型会话
    close_openai_instance(openai_instance)
    print(f"ERROR - 读取input文件时发生错误: {e}")
    exit(1)

# 根据type字段类型，进入不同的分支，并调用工具
tool_type = args.type
if tool_type == "var":
    res = var_translation(code, LLM_config.model, openai_instance)
elif tool_type == "fn":
    res = function_translation(code, LLM_config.model, openai_instance)
elif tool_type == "fix":
    # 加错误信息
    try:
        with open(args.error, 'r', encoding='utf-8') as file:
            error = file.read()
    except Exception as e:
        # 关闭大模型会话
        close_openai_instance(openai_instance)
        print(f"ERROR - 读取error文件时发生错误: {e}")
        exit(1)
    res = syntax_fixing(code, error, LLM_config.model, openai_instance)
else:
    # 关闭大模型会话
    close_openai_instance(openai_instance)
    print(f"调用工具的类型出错，只允许以下值： var/fn/fix（分别代表变量翻译/函数翻译/语法修复）")
    exit(1)

# 关闭大模型会话
close_openai_instance(openai_instance)

# 将结果写入输入路径
try:
    with open(args.output, 'w', encoding='utf-8') as file:
        file.write(res)
        print(f"翻译/修复结果已写入 {args.output}")
except Exception as e:
    print(f"ERROR - 写入output文件时发生错误: {e}")
    exit(1)