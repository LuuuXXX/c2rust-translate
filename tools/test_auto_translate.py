#!/usr/bin/env python3
"""
测试脚本：验证 auto_translate.py 的基本功能

注意：这是一个单元测试脚本，用于验证工具的各个组件是否正常工作。
完整的集成测试需要实际的C2Rust项目环境。
"""

import sys
import os
from pathlib import Path
import tempfile
import shutil

# 添加tools目录到Python路径
sys.path.insert(0, str(Path(__file__).parent))

from auto_translate import AutoTranslate


def test_initialization():
    """测试AutoTranslate类的初始化"""
    print("测试1: AutoTranslate类初始化...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        at = AutoTranslate("test_feature", tmpdir)
        
        assert at.feature == "test_feature"
        assert at.project_root == Path(tmpdir)
        assert at.feature_dir == Path(tmpdir) / "test_feature"
        assert at.rust_dir == Path(tmpdir) / "test_feature" / "rust"
        
        print("✓ 初始化测试通过")


def test_extract_type_from_filename():
    """测试从文件名提取类型"""
    print("\n测试2: 从文件名提取类型...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        at = AutoTranslate("test_feature", tmpdir)
        
        assert at.extract_type_from_filename("var_test.rs") == "var"
        assert at.extract_type_from_filename("fun_test.rs") == "fn"
        assert at.extract_type_from_filename("other_test.rs") is None
        
        print("✓ 类型提取测试通过")


def test_scan_empty_rs_files():
    """测试扫描空rs文件"""
    print("\n测试3: 扫描空rs文件...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        at = AutoTranslate("test_feature", tmpdir)
        
        # 创建测试目录结构
        rust_dir = Path(tmpdir) / "test_feature" / "rust"
        rust_dir.mkdir(parents=True, exist_ok=True)
        
        # 创建测试文件
        empty_file1 = rust_dir / "var_empty1.rs"
        empty_file2 = rust_dir / "fun_empty2.rs"
        non_empty_file = rust_dir / "var_nonempty.rs"
        
        empty_file1.write_text("", encoding='utf-8')
        empty_file2.write_text("   \n  \n", encoding='utf-8')  # 只有空白
        non_empty_file.write_text("fn test() {}", encoding='utf-8')
        
        # 扫描空文件
        empty_files = at.scan_empty_rs_files()
        
        assert len(empty_files) == 2
        assert empty_file1 in empty_files
        assert empty_file2 in empty_files
        assert non_empty_file not in empty_files
        
        print("✓ 空文件扫描测试通过")


def test_find_corresponding_c_file():
    """测试查找对应的C文件"""
    print("\n测试4: 查找对应的C文件...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        at = AutoTranslate("test_feature", tmpdir)
        
        # 创建测试目录结构
        rust_dir = Path(tmpdir) / "test_feature" / "rust"
        rust_dir.mkdir(parents=True, exist_ok=True)
        
        # 创建测试文件
        rs_file = rust_dir / "var_test.rs"
        c_file = rust_dir / "var_test.c"
        
        rs_file.write_text("", encoding='utf-8')
        c_file.write_text("int test;", encoding='utf-8')
        
        # 查找C文件
        found_c_file = at.find_corresponding_c_file(rs_file)
        
        assert found_c_file == c_file
        
        # 测试不存在的情况
        rs_file2 = rust_dir / "var_noexist.rs"
        rs_file2.write_text("", encoding='utf-8')
        found_c_file2 = at.find_corresponding_c_file(rs_file2)
        
        assert found_c_file2 is None
        
        print("✓ C文件查找测试通过")


def test_extract_error_file():
    """测试错误文件提取"""
    print("\n测试5: 错误文件提取...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        at = AutoTranslate("test_feature", tmpdir)
        
        rust_dir = Path(tmpdir) / "test_feature" / "rust"
        rust_dir.mkdir(parents=True, exist_ok=True)
        
        rs_file = rust_dir / "var_test.rs"
        
        # 测试包含文件名的错误信息
        error_msg1 = "error: cannot find value `x` in this scope\n --> var_test.rs:10:5"
        assert at.extract_error_file(error_msg1, rs_file) == True
        
        # 测试不包含文件名的错误信息
        error_msg2 = "error: cannot find value `x` in this scope\n --> other_file.rs:10:5"
        assert at.extract_error_file(error_msg2, rs_file) == False
        
        print("✓ 错误文件提取测试通过")


def run_all_tests():
    """运行所有测试"""
    print("="*60)
    print("开始运行 auto_translate.py 单元测试")
    print("="*60)
    
    try:
        test_initialization()
        test_extract_type_from_filename()
        test_scan_empty_rs_files()
        test_find_corresponding_c_file()
        test_extract_error_file()
        
        print("\n" + "="*60)
        print("所有测试通过！✓")
        print("="*60)
        return 0
        
    except AssertionError as e:
        print(f"\n✗ 测试失败: {e}")
        return 1
    except Exception as e:
        print(f"\n✗ 测试出错: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == '__main__':
    sys.exit(run_all_tests())
