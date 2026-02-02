from LLM_calling import call_llm_api
from parse_LLM_output import filter_model_response
from LLM_calling import load_LLM_config, init_openai_instance # 这两个函数只用在该文件的示例中，实际项目中应在初始化阶段完成

def enum_translation(C_enum_code, model, openai_instance):
    # 构建 prompt
    prompt = f"""
    你是一个代码翻译专家，请将以下的C枚举翻译为Rust。C枚举的每一个成员应该翻译为Rust的一个全局常量，该全局常量的值即为其对应的枚举的成员的值。

    输入的C函数：
    {C_enum_code}

    输出格式：
    Rust翻译结果:
    pub const a: i32 = 0;
    pub const b: i32 = -1;

    注意：
    - 严格按输出格式输出，无需其他任何内容
    - 确保C枚举的成员变量与翻译后的Rust全局常量的名称是完全一致的（特别注意大小写）
    - 确保翻译了C枚举中的所有的成员变量
    - 确保所有的Rust全局常量都添加了pub const
    - 确保所有的Rust全局常量的类型都是正确的（可通过Rust编译的）
    """

    # todo： 拼接翻译规则与约束的prompt

    # 调用大模型
    output = call_llm_api(prompt, model, openai_instance)

    res = filter_model_response(output)

    return res

# 示例用法
if __name__ == "__main__":
    c_enum_code = """
        enum Status {
            IDLE = 0,
            SUCCESS = 1,
            ERROR_IO = -1,
            ERROR_MEMORY = -2,
            MAX_ERROR = ERROR_MEMORY  // 重复值
        };
    """

    # 加载LLM配置信息
    config_file_path = "config.toml"
    LLM_config = load_LLM_config(config_file_path)

    # 创建与大模型平台的api会话实例
    openai_instance = init_openai_instance(LLM_config)

    output = enum_translation(c_enum_code, LLM_config.model, openai_instance)
    print("output:")
    print(output)