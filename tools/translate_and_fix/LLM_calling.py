import openai
import httpx
import toml
import os

class LLM_config:
    def __init__(self, model, base_url, api_key, temperature, top_p, seed, max_retries, timeout):
        self.model = model
        self.base_url = base_url
        self.api_key = api_key
        self.temperature = temperature
        self.top_p = top_p
        self.seed = seed
        self.max_retries = max_retries
        self.timeout = timeout

    def __repr__(self):
        return (f"LLM_config(model={self.model}, base_url={self.base_url}, "
                f"api_key={self.api_key}, temperature={self.temperature}, "
                f"top_p={self.top_p}, seed={self.seed}, max_retries={self.max_retries}, "
                f"timeout={self.timeout})")


def load_LLM_config(file_path):
    """
    从config.toml文件中读取大模型配置并创建LLM_config对象

    Args:
        file_path (str): config.toml文件的路径

    Returns:
        LLM_config: 包含大模型配置的对象

    Raises:
        FileNotFoundError: 如果文件不存在
        toml.TomlDecodeError: 如果TOML格式解析失败
        KeyError: 如果缺少必要的配置项
    """
    # 检查文件是否存在
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"配置文件不存在: {file_path}")

    try:
        # 读取并解析TOML文件
        with open(file_path, 'r', encoding='utf-8') as f:
            config_data = toml.load(f)

        # 检查是否存在model配置段
        if 'model' not in config_data:
            raise KeyError("配置文件中缺少[model]字段")

        model_config = config_data['model']

        # 提取配置项，注意处理空格和类型转换
        model = model_config.get('model', '').strip()
        base_url = model_config.get('base_url', '').strip()  # 去除多余空格
        api_key = model_config.get('api_key', '').strip()

        # 数值类型配置，提供默认值以防缺失
        temperature = float(model_config.get('temperature', 0.0))
        top_p = float(model_config.get('top_p', 1.0))
        seed = int(model_config.get('seed', 42))
        max_retries = int(model_config.get('max_retries', 10))
        timeout = int(model_config.get('timeout', 500))

        # 检查必要配置项是否缺失
        required_fields = ['model', 'base_url', 'api_key']
        missing_fields = [field for field in required_fields if not locals()[field]]
        if missing_fields:
            raise KeyError(f"缺少必要的配置项: {', '.join(missing_fields)}")

        # 创建并返回LLM_config对象
        return LLM_config(
            model=model,
            base_url=base_url,
            api_key=api_key,
            temperature=temperature,
            top_p=top_p,
            seed=seed,
            max_retries=max_retries,
            timeout=timeout
        )

    except toml.TomlDecodeError as e:
        raise ValueError(f"TOML文件格式错误: {e}") from e
    except Exception as e:
        raise RuntimeError(f"读取配置文件时出错: {e}") from e

# def load_LLM_config(config_file_path):
#     with open(config_file_path, 'r') as file:
#         config_data = yaml.safe_load(file)
#
#     # 提取配置信息
#     translator_config = config_data.get('translator', {})
#     model = translator_config.get('model')
#     base_url = translator_config.get('base_url')
#     api_key = translator_config.get('api_key')
#     temperature = translator_config.get('temperature')
#     top_p = translator_config.get('top_p')
#     seed = translator_config.get('seed')
#     max_retries = translator_config.get('max_retries')
#     timeout = translator_config.get('timeout')
#
#     # 创建 LLM_config 对象
#     llm_config = LLM_config(
#         model=model,
#         base_url=base_url,
#         api_key=api_key,
#         temperature=temperature,
#         top_p=top_p,
#         seed=seed,
#         max_retries=max_retries,
#         timeout=timeout
#     )
#
#     return llm_config

# return a new openai instance
def init_openai_instance(LLM_config):
    # 获取当前进程的专属KEY
    api_key = LLM_config.api_key
    base_url = LLM_config.base_url

    # 提供持久化连接的功能，可以重复使用相同的TCP连接，从而提高性能
    clean_http_client = httpx.Client(verify=False)

    # set up OpenAI instance with custom parameters
    openai_instance = openai.OpenAI(
        base_url=base_url,
        api_key=api_key,
        http_client=clean_http_client
    )

    return openai_instance

# 调用LLM修复Rust编译/语义错误
def call_llm_api(prompt, model, openai_instance):
    response = openai_instance.chat.completions.create(
        model=model,
        messages=[
            {"role": "system",
             "content": "你是一个Rust语言的专家，你擅长将C代码翻译为Rust代码，以及修复Rust代码中的语法语义错误."},
            {"role": "user", "content": prompt}
        ]
    )

    # Extract content from LLM response
    response_content = response.choices[0].message.content
    return response_content

# 示例用法
if __name__ == "__main__":
    config_file_path = "config.toml"
    llm_config = load_LLM_config(config_file_path)
    print("llm_config:")
    print(llm_config)

    openai_instance = init_openai_instance(llm_config)
    print("创建了一个api会话")
    print(type(openai_instance))

    prompt = "Hello!"
    response_content = call_llm_api(prompt, llm_config.model, openai_instance)
    print("response_content:")
    print(response_content)