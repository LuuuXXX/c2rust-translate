from LLM_calling import call_llm_api
from parse_LLM_output import filter_model_response
from LLM_calling import load_LLM_config, init_openai_instance

def var_translation(C_var_code, model, openai_instance):
    # 构建 prompt
    prompt = f"""
    你是一个代码翻译专家，请将以下的C全局变量翻译为Rust全局变量。

    输入的C全局变量：
    {C_var_code}

    输出格式：
    Rust翻译结果:
    // 在这里写出翻译出的Rust全局变量

    注意：
    - 严格按输出格式输出，无需其他任何内容
    - 确保C全局变量与翻译后的Rust全局变量的名称是完全一致的（特别注意大小写）
    - 确保Rust全局变量的类型是正确的（可通过Rust编译的），并且是与C全局变量的类型是对应的
    - 确保Rust全局变量添加了pub关键字，但是对于C静态变量(static)，翻译结果不应添加pub关键字
    - 如果C全局变量有const关键字，那么翻译结果也应添加const关键字；否则，翻译结果应添加mut关键字
    - 对于C静态变量(static)，翻译结果应该添加static关键字
    - 对于指针类型，使用*mut或*const；对于数组，使用固定大小数组
    - 对于未初始化的变量，使用unsafe {{std::mem::zeroed() }}进行初始化
    - 如果C变量的初始化值是一个表达式，那么翻译结果也应该通过翻译该表达式来赋值，来确保可维护性
    - 如果输入的C变量有注释，那么翻译结果应该将这些注释翻译为与Rust翻译结果对应的注释
    """

    # 调用大模型
    output = call_llm_api(prompt, model, openai_instance)

    res = filter_model_response(output)

    return res

# ====================== 测试用例 ======================
def run_all_tests():
    """运行所有测试用例"""
    tests = [
        # 测试用例1：基本整型
        {
            "name": "基本整型变量",
            "c_code": "int counter = 0;",
            "explanation": "简单的整型全局变量，带初始化",
            "expected": """
pub static mut counter: i32 = 0;
            """
        },

        # 测试用例2：浮点类型
        {
            "name": "浮点类型变量",
            "c_code": "double pi = 3.1415926535;",
            "explanation": "双精度浮点数全局变量",
            "expected": """
pub static mut pi: f64 = 3.1415926535;
            """
        },

        # 测试用例3：字符类型
        {
            "name": "字符类型变量",
            "c_code": "char default_char = 'A';",
            "explanation": "字符类型全局变量",
            "expected": """
pub static mut default_char: u8 = b'A';
            """
        },

        # 测试用例4：未初始化变量
        {
            "name": "未初始化变量",
            "c_code": "float temperature;",
            "explanation": "未初始化的浮点型全局变量，Rust需要安全初始化",
            "expected": """
pub static mut temperature: f32 = unsafe { std::mem::zeroed() };
            """
        },

        # 测试用例5：数组类型
        {
            "name": "一维数组",
            "c_code": "int numbers[10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};",
            "explanation": "固定大小的一维整型数组，带初始化",
            "expected": """
pub static mut numbers: [i32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            """
        },

        # 测试用例6：多维数组
        {
            "name": "二维数组",
            "c_code": "int matrix[3][3] = {{1, 2, 3}, {4, 5, 6}, {7, 8, 9}};",
            "explanation": "二维整型数组，Rust使用嵌套数组语法",
            "expected": """
pub static mut matrix: [[i32; 3]; 3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
            """
        },

        # 测试用例7：指针类型
        {
            "name": "指针类型变量",
            "c_code": "int* data_ptr = NULL;",
            "explanation": "整型指针，初始化为NULL，Rust使用空指针",
            "expected": """
pub static mut data_ptr: *mut i32 = std::ptr::null_mut();
            """
        },

        # 测试用例8：字符串常量
        {
            "name": "字符串常量",
            "c_code": "const char* welcome_msg = \"Hello, World!\";",
            "explanation": "字符串指针，Rust需要处理字符串字面量和生命周期",
            "expected": """
pub const welcome_msg: *const u8 = b\"Hello, World!\\0\".as_ptr();
            """
        },

        # 测试用例9：常量全局变量
        {
            "name": "常量全局变量",
            "c_code": "const int MAX_SIZE = 100;",
            "explanation": "const修饰的全局变量，Rust中应为不可变静态变量",
            "expected": """
pub const MAX_SIZE: i32 = 100;
            """
        },

        # 测试用例10：静态全局变量
        {
            "name": "静态全局变量",
            "c_code": "static int private_counter = 0;",
            "explanation": "static修饰的全局变量，Rust中应考虑模块级可见性",
            "expected": """
static mut private_counter: i32 = 0;
            """
        },

        # 测试用例11：联合体类型
        {
            "name": "联合体类型变量",
            "c_code": "union Data { int i; float f; char str[20]; }; union Data current_data;",
            "explanation": "联合体类型的全局变量，Rust中使用union关键字",
            "expected": """
#[repr(C)]
pub union Data {
    pub i: i32,
    pub f: f32,
    pub str: [u8; 20],
}

pub static mut current_data: Data = unsafe { std::mem::zeroed() };
            """
        },

        # 测试用例12：复杂初始化
        {
            "name": "复杂初始化表达式",
            "c_code": "int factorial = 1 * 2 * 3 * 4 * 5;",
            "explanation": "使用复杂表达式初始化的变量，Rust可以处理常量表达式",
            "expected": """
pub static mut factorial: i32 = 1 * 2 * 3 * 4 * 5;
            """
        },

        # 测试用例13：多个变量声明
        {
            "name": "多个变量在同一行",
            "c_code": "int a = 1, b = 2, c = 3;",
            "explanation": "同一行声明多个变量，Rust需要拆分为单独的声明",
            "expected": """
pub static mut a: i32 = 1;
pub static mut b: i32 = 2;
pub static mut c: i32 = 3;
            """
        },

        # 测试用例14：函数指针
        {
            "name": "函数指针变量",
            "c_code": "void (*callback)(int) = NULL;",
            "explanation": "函数指针类型的全局变量，Rust使用函数指针类型",
            "expected": """
pub static mut callback: Option<unsafe extern \"C\" fn(i32)> = None;
            """
        },

        # 测试用例15：带注释的变量
        {
            "name": "带注释的变量声明",
            "c_code": """
            // This is a configuration variable
            int config_value = 42;  // Default configuration
            """,
            "explanation": "带注释的变量声明，Rust应保留注释或生成清晰的代码",
            "expected": """
// This is a configuration variable
pub static mut config_value: i32 = 42;  // Default configuration
            """
        },

        # 测试用例16：位域（特殊处理）
        {
            "name": "位域（需要特殊处理）",
            "c_code": "struct Flags { unsigned int flag1 : 1; unsigned int flag2 : 1; }; struct Flags status_flags;",
            "explanation": "位域结构体，Rust不支持位域，需要手动实现位操作",
            "expected": """
#[repr(C)]
pub struct Flags {
    pub _bitfield: u32,
}

impl Flags {
    pub fn new() -> Self {
        Self { _bitfield: 0 }
    }

    pub fn get_flag1(&self) -> bool {
        (self._bitfield & 0x1) != 0
    }

    pub fn set_flag1(&mut self, value: bool) {
        if value {
            self._bitfield |= 0x1;
        } else {
            self._bitfield &= !0x1;
        }
    }

    pub fn get_flag2(&self) -> bool {
        (self._bitfield & 0x2) != 0
    }

    pub fn set_flag2(&mut self, value: bool) {
        if value {
            self._bitfield |= 0x2;
        } else {
            self._bitfield &= !0x2;
        }
    }
}

pub static mut status_flags: Flags = Flags::new();
            """
        }
    ]

    # 运行测试
    print("=== 开始运行所有测试用例 ===\n")

    # 加载LLM配置信息（实际项目中应在初始化阶段完成）
    config_file_path = "config.toml"
    try:
        LLM_config = load_LLM_config(config_file_path)
        openai_instance = init_openai_instance(LLM_config)

        for i, test in enumerate(tests, 1):
            print(f"【测试用例 {i}/{len(tests)}】{test['name']}")
            print(f"说明: {test['explanation']}")
            print("C代码:")
            print(test['c_code'])
            print("\n期望的Rust翻译结果:")
            print(test['expected'].strip())

            # 调用翻译函数
            print("\n实际翻译结果:")
            result = var_translation(test['c_code'], LLM_config.model, openai_instance)
            print(result)

            print("-" * 60 + "\n")

    except Exception as e:
        print(f"测试过程中出错: {e}")


# 示例用法
if __name__ == "__main__":
    # 运行所有测试用例
    run_all_tests()