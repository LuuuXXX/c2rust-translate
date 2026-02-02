from LLM_calling import call_llm_api
from parse_LLM_output import filter_model_response
from LLM_calling import load_LLM_config, init_openai_instance # 这两个函数只用在该文件的示例中，实际项目中应在初始化阶段完成

def function_semantic_fixing(c_function_code, rust_function_code, c_test_case_code, model, openai_instance):
    # 构建 prompt
    prompt = f"""
    你是一个代码修复专家，我们之前将一个C函数：
    {c_function_code}
    翻译为了以下的Rust函数：
    {rust_function_code}
    
    我们用这个Rust函数替换了这个C函数后，对C与Rust进行联合编译与测试，
    但发现它没有通过原有的所有C测试用例。请修复这个Rust函数，使其能够通过以下C测试用例：
    {c_test_case_code}
    
    输出格式：
    修复后的Rust函数代码（严格按输出格式示例的格式输出函数代码，无需其他说明）
    
    输出格式示例：
    Rust翻译结果:
    #[unsafe(no_mangle)]
    pub extern "C" unsafe fn foo() {{
        // Rust 代码
    }}
    """

    # todo： 拼接翻译规则与约束的prompt

    # 调用大模型
    output = call_llm_api(prompt, model, openai_instance)

    res = filter_model_response(output)

    return res

# 示例用法
if __name__ == "__main__":
    c_function_code = """
    void my_function() {
        printf("Hello from C!");
    }
    """

    rust_function_code = """
    fn my_function() {
        println!("Hello from Rust!")
    }
    """

    c_test_case_code = """
    int main() {
        my_function();
        return 0;
    }
    """

    # 加载LLM配置信息
    config_file_path = "config.toml"
    LLM_config = load_LLM_config(config_file_path)

    # 创建与大模型平台的api会话实例
    openai_instance = init_openai_instance(LLM_config)

    fixed_rust_function = function_semantic_fixing(c_function_code, rust_function_code, c_test_case_code, LLM_config.model, openai_instance)
    print("修复后的 Rust 函数:")
    print(fixed_rust_function)