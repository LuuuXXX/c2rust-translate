#!/usr/bin/env python3
"""
自动化C到Rust翻译工作流工具

该工具自动管理C到Rust的翻译流程，包括：
- 初始化检查
- 翻译C代码到Rust
- 编译修复
- 代码分析更新
- 混合构建和测试
"""

import argparse
import os
import sys
import subprocess
import logging
from pathlib import Path
from typing import List, Tuple, Optional


# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    datefmt='%Y-%m-%d %H:%M:%S'
)
logger = logging.getLogger(__name__)


class AutoTranslate:
    """自动翻译工作流管理类"""
    
    def __init__(self, feature: str, project_root: str = None):
        """
        初始化自动翻译工作流
        
        Args:
            feature: 功能名称
            project_root: 项目根目录路径，如果为None则使用当前工作目录
        """
        self.feature = feature
        self.project_root = Path(project_root) if project_root else Path.cwd()
        self.feature_dir = self.project_root / feature
        self.rust_dir = self.feature_dir / "rust"
        self.c2rust_dir = self.project_root / ".c2rust"
        self.config_file = self.c2rust_dir / "config.toml"
        self.translate_tool_dir = Path(__file__).parent / "translate_and_fix"
        self.translate_tool = self.translate_tool_dir / "translate_and_fix.py"
        
    def run_command(self, cmd: List[str], cwd: str = None, 
                   capture_output: bool = True, check: bool = True,
                   env: dict = None) -> Tuple[int, str, str]:
        """
        执行外部命令
        
        Args:
            cmd: 命令列表
            cwd: 工作目录
            capture_output: 是否捕获输出
            check: 是否检查返回码
            env: 环境变量字典
            
        Returns:
            (返回码, stdout, stderr)
        """
        try:
            logger.info(f"执行命令: {' '.join(cmd)}")
            if cwd:
                logger.info(f"工作目录: {cwd}")
            
            # 准备环境变量
            cmd_env = os.environ.copy()
            if env:
                cmd_env.update(env)
            
            result = subprocess.run(
                cmd,
                cwd=cwd,
                capture_output=capture_output,
                text=True,
                env=cmd_env
            )
            
            if check and result.returncode != 0:
                logger.error(f"命令执行失败，返回码: {result.returncode}")
                if capture_output:
                    logger.error(f"错误输出: {result.stderr}")
            
            return result.returncode, result.stdout if capture_output else "", result.stderr if capture_output else ""
        
        except Exception as e:
            logger.error(f"执行命令时发生异常: {e}")
            raise
    
    def check_and_init(self) -> bool:
        """
        检查并初始化feature/rust目录
        
        Returns:
            初始化成功返回True，失败返回False
        """
        logger.info(f"检查 {self.rust_dir} 目录是否存在")
        
        if self.rust_dir.exists():
            logger.info(f"目录 {self.rust_dir} 已存在")
            return True
        
        logger.info(f"目录 {self.rust_dir} 不存在，执行初始化")
        
        # 执行 code-analyse --init
        returncode, stdout, stderr = self.run_command(
            ["code-analyse", "--feature", self.feature, "--init"],
            cwd=str(self.project_root),
            check=False
        )
        
        if returncode != 0:
            logger.error(f"code-analyse --init 执行失败")
            logger.error(f"错误信息: {stderr}")
            return False
        
        # 再次检查rust目录是否存在
        if not self.rust_dir.exists():
            logger.error(f"code-analyse --init 执行成功但 {self.rust_dir} 目录仍不存在")
            return False
        
        # 提交变更
        logger.info("提交初始化的文件")
        self.git_commit(f"Initialize {self.feature} directory with code-analyse")
        
        return True
    
    def git_commit(self, message: str) -> bool:
        """
        提交当前变更
        
        Args:
            message: 提交信息
            
        Returns:
            提交成功返回True，失败返回False
        """
        try:
            # 添加所有变更
            self.run_command(["git", "add", "."], cwd=str(self.project_root))
            
            # 检查是否有变更需要提交
            returncode, stdout, stderr = self.run_command(
                ["git", "diff", "--cached", "--quiet"],
                cwd=str(self.project_root),
                check=False
            )
            
            # 如果有变更（返回码非0），则提交
            if returncode != 0:
                self.run_command(
                    ["git", "commit", "-m", message],
                    cwd=str(self.project_root)
                )
                logger.info(f"已提交变更: {message}")
            else:
                logger.info("没有变更需要提交")
            
            return True
        
        except Exception as e:
            logger.error(f"Git提交失败: {e}")
            return False
    
    def scan_empty_rs_files(self) -> List[Path]:
        """
        扫描rust目录下的空rs文件
        
        Returns:
            空rs文件路径列表
        """
        empty_files = []
        
        if not self.rust_dir.exists():
            logger.warning(f"目录 {self.rust_dir} 不存在")
            return empty_files
        
        for rs_file in self.rust_dir.rglob("*.rs"):
            if rs_file.is_file():
                # 检查文件是否为空或只包含空白字符
                content = rs_file.read_text(encoding='utf-8').strip()
                if not content:
                    empty_files.append(rs_file)
                    logger.info(f"发现空文件: {rs_file}")
        
        return empty_files
    
    def extract_type_from_filename(self, filename: str) -> Optional[str]:
        """
        从文件名提取类型
        
        Args:
            filename: 文件名
            
        Returns:
            'var' 或 'fn'，如果无法识别则返回None
        """
        if filename.startswith("var_"):
            return "var"
        elif filename.startswith("fun_"):
            return "fn"
        else:
            logger.warning(f"无法从文件名 {filename} 识别类型")
            return None
    
    def find_corresponding_c_file(self, rs_file: Path) -> Optional[Path]:
        """
        查找对应的C文件
        
        Args:
            rs_file: Rust文件路径
            
        Returns:
            对应的C文件路径，如果不存在则返回None
        """
        c_file = rs_file.with_suffix('.c')
        if c_file.exists():
            return c_file
        else:
            logger.warning(f"找不到对应的C文件: {c_file}")
            return None
    
    def translate_file(self, c_file: Path, rs_file: Path, file_type: str) -> bool:
        """
        翻译C文件到Rust
        
        Args:
            c_file: C文件路径
            rs_file: Rust文件路径
            file_type: 文件类型 ('var' 或 'fn')
            
        Returns:
            翻译成功返回True，失败返回False
        """
        logger.info(f"开始翻译 {c_file} -> {rs_file} (类型: {file_type})")
        
        # 调用translate_and_fix.py
        returncode, stdout, stderr = self.run_command(
            [
                "python3",
                str(self.translate_tool),
                "--config", str(self.config_file),
                "--type", file_type,
                "--code", str(c_file),
                "--output", str(rs_file)
            ],
            cwd=str(self.project_root),
            check=False
        )
        
        if returncode != 0:
            logger.error(f"翻译失败: {stderr}")
            return False
        
        # 检查输出文件是否为空
        if not rs_file.exists() or not rs_file.read_text(encoding='utf-8').strip():
            logger.error(f"翻译成功但输出文件为空: {rs_file}")
            return False
        
        logger.info(f"翻译成功: {rs_file}")
        return True
    
    def cargo_build(self) -> Tuple[bool, str]:
        """
        在rust目录执行cargo build
        
        Returns:
            (构建成功, 错误信息)
        """
        logger.info("执行 cargo build")
        
        returncode, stdout, stderr = self.run_command(
            ["cargo", "build"],
            cwd=str(self.rust_dir),
            check=False
        )
        
        if returncode == 0:
            logger.info("cargo build 成功")
            return True, ""
        else:
            logger.warning("cargo build 失败")
            # 返回完整的错误信息（包括stdout和stderr）
            error_message = stdout + "\n" + stderr
            return False, error_message
    
    def extract_error_file(self, error_message: str, rs_file: Path) -> bool:
        """
        检查错误信息是否与指定的rs文件相关
        
        Args:
            error_message: 错误信息
            rs_file: Rust文件路径
            
        Returns:
            如果错误与该文件相关返回True
        """
        # 简单检查：错误信息中是否包含文件名
        return str(rs_file.name) in error_message or str(rs_file) in error_message
    
    def fix_compilation_errors(self, rs_file: Path, error_message: str) -> bool:
        """
        修复编译错误
        
        Args:
            rs_file: Rust文件路径
            error_message: 错误信息
            
        Returns:
            修复成功返回True，失败返回False
        """
        logger.info(f"开始修复编译错误: {rs_file}")
        
        # 创建临时错误文件
        error_file = Path("/tmp/error_message.txt")
        error_file.write_text(error_message, encoding='utf-8')
        
        # 调用translate_and_fix.py进行修复
        returncode, stdout, stderr = self.run_command(
            [
                "python3",
                str(self.translate_tool),
                "--config", str(self.config_file),
                "--type", "fix",
                "--code", str(rs_file),
                "--output", str(rs_file),
                "--error", str(error_file)
            ],
            cwd=str(self.project_root),
            check=False
        )
        
        # 清理临时文件
        error_file.unlink(missing_ok=True)
        
        if returncode != 0:
            logger.error(f"修复失败: {stderr}")
            return False
        
        # 检查输出文件是否为空
        if not rs_file.exists() or not rs_file.read_text(encoding='utf-8').strip():
            logger.error(f"修复成功但输出文件为空: {rs_file}")
            return False
        
        logger.info(f"修复成功: {rs_file}")
        return True
    
    def code_analyse_update(self) -> bool:
        """
        执行code-analyse --update
        
        Returns:
            更新成功返回True，失败返回False
        """
        logger.info(f"执行 code-analyse --update")
        
        returncode, stdout, stderr = self.run_command(
            ["code-analyse", "--feature", self.feature, "--update"],
            cwd=str(self.project_root),
            check=False
        )
        
        if returncode != 0:
            logger.error(f"code-analyse --update 执行失败")
            logger.error(f"错误信息: {stderr}")
            return False
        
        logger.info("code-analyse --update 执行成功")
        return True
    
    def get_build_commands(self) -> Tuple[List[str], List[str], List[str]]:
        """
        从配置文件获取构建命令
        
        Returns:
            (clean命令列表, build命令列表, test命令列表)
        """
        clean_cmds = []
        build_cmds = []
        test_cmds = []
        
        try:
            # 使用c2rust-config获取命令
            returncode, stdout, stderr = self.run_command(
                ["c2rust-config", "config", "--list", "clean"],
                cwd=str(self.project_root),
                check=False
            )
            if returncode == 0 and stdout.strip():
                clean_cmds = [line.strip() for line in stdout.strip().split('\n') if line.strip()]
            
            returncode, stdout, stderr = self.run_command(
                ["c2rust-config", "config", "--list", "build"],
                cwd=str(self.project_root),
                check=False
            )
            if returncode == 0 and stdout.strip():
                build_cmds = [line.strip() for line in stdout.strip().split('\n') if line.strip()]
            
            returncode, stdout, stderr = self.run_command(
                ["c2rust-config", "config", "--list", "test"],
                cwd=str(self.project_root),
                check=False
            )
            if returncode == 0 and stdout.strip():
                test_cmds = [line.strip() for line in stdout.strip().split('\n') if line.strip()]
        
        except Exception as e:
            logger.warning(f"获取构建命令时出错: {e}")
        
        return clean_cmds, build_cmds, test_cmds
    
    def execute_build_commands(self, hybrid_lib_path: str = None) -> bool:
        """
        执行构建命令
        
        Args:
            hybrid_lib_path: 混合构建库路径
            
        Returns:
            执行成功返回True，失败返回False
        """
        clean_cmds, build_cmds, test_cmds = self.get_build_commands()
        
        # 准备环境变量
        env = {}
        if hybrid_lib_path:
            env['LD_PRELOAD'] = hybrid_lib_path
        env['C2RUST_FEATURE_ROOT'] = str(self.feature_dir)
        
        # 执行clean命令
        for cmd in clean_cmds:
            logger.info(f"执行clean命令: {cmd}")
            returncode, stdout, stderr = self.run_command(
                ["c2rust-clean", "clean", "--"] + cmd.split(),
                cwd=str(self.project_root),
                check=False,
                env=env
            )
            if returncode != 0:
                logger.error(f"clean命令执行失败: {cmd}")
                logger.error(f"请手工处理后重试")
                return False
        
        # 执行build命令
        for cmd in build_cmds:
            logger.info(f"执行build命令: {cmd}")
            returncode, stdout, stderr = self.run_command(
                ["c2rust-build", "build", "--"] + cmd.split(),
                cwd=str(self.project_root),
                check=False,
                env=env
            )
            if returncode != 0:
                logger.error(f"build命令执行失败: {cmd}")
                logger.error(f"请手工处理后重试")
                return False
        
        # 执行test命令
        for cmd in test_cmds:
            logger.info(f"执行test命令: {cmd}")
            returncode, stdout, stderr = self.run_command(
                ["c2rust-test", "test", "--"] + cmd.split(),
                cwd=str(self.project_root),
                check=False,
                env=env
            )
            if returncode != 0:
                logger.error(f"test命令执行失败: {cmd}")
                logger.error(f"请手工处理后重试")
                return False
        
        logger.info("所有构建命令执行成功")
        return True
    
    def process_empty_file(self, rs_file: Path) -> bool:
        """
        处理单个空的rs文件
        
        Args:
            rs_file: Rust文件路径
            
        Returns:
            处理成功返回True，失败返回False
        """
        logger.info(f"处理空文件: {rs_file}")
        
        # 提取类型
        file_type = self.extract_type_from_filename(rs_file.name)
        if not file_type:
            logger.error(f"无法识别文件类型: {rs_file}")
            return False
        
        # 查找对应的C文件
        c_file = self.find_corresponding_c_file(rs_file)
        if not c_file:
            logger.error(f"找不到对应的C文件")
            
            # 询问用户是否需要重新初始化
            response = input("工程可能被破坏，是否需要执行 code-analyse --init? (y/n): ")
            if response.lower() == 'y':
                if not self.check_and_init():
                    logger.error("初始化失败")
                    return False
                # 重新查找C文件
                c_file = self.find_corresponding_c_file(rs_file)
                if not c_file:
                    logger.error("初始化后仍找不到C文件")
                    return False
            else:
                logger.info("用户选择不执行初始化，退出")
                return False
        
        # 翻译文件
        if not self.translate_file(c_file, rs_file, file_type):
            logger.error(f"翻译失败: {rs_file}")
            return False
        
        # 编译与修复循环
        max_fix_attempts = 5  # 最大修复尝试次数
        for attempt in range(max_fix_attempts):
            success, error_message = self.cargo_build()
            
            if success:
                logger.info("编译成功")
                break
            
            # 检查错误是否与当前文件相关
            if not self.extract_error_file(error_message, rs_file):
                logger.warning(f"编译错误可能与其他文件相关，跳过修复")
                # 即使有其他文件的错误，我们也认为当前文件已处理完成
                break
            
            logger.info(f"尝试修复编译错误 (第 {attempt + 1}/{max_fix_attempts} 次)")
            
            if not self.fix_compilation_errors(rs_file, error_message):
                logger.error(f"修复失败")
                return False
        else:
            # 达到最大尝试次数
            logger.error(f"达到最大修复尝试次数 ({max_fix_attempts})，仍有编译错误")
            return False
        
        # 提交翻译成功的文件
        self.git_commit(f"Translate {rs_file.name}")
        
        # 更新代码分析
        if not self.code_analyse_update():
            logger.error("code-analyse --update 失败")
            return False
        
        # 提交更新的文件
        self.git_commit(f"Update code analysis for {rs_file.name}")
        
        # 执行混合构建
        # 注意：这里需要从配置或环境变量中获取混合构建库路径
        # 暂时不传递混合构建库路径，如果需要可以通过命令行参数或环境变量传入
        hybrid_lib_path = os.environ.get('C2RUST_HYBRID_LIB')
        if not self.execute_build_commands(hybrid_lib_path):
            logger.error("混合构建失败")
            return False
        
        logger.info(f"文件处理完成: {rs_file}")
        return True
    
    def run(self) -> int:
        """
        运行自动翻译工作流
        
        Returns:
            退出码，0表示成功，非0表示失败
        """
        logger.info("="*60)
        logger.info("开始自动化C到Rust翻译工作流")
        logger.info(f"Feature: {self.feature}")
        logger.info(f"项目根目录: {self.project_root}")
        logger.info("="*60)
        
        # 1. 初始化检查
        if not self.check_and_init():
            logger.error("初始化检查失败，退出")
            return 1
        
        # 2. 主循环：处理空的rs文件
        while True:
            # 扫描空文件
            empty_files = self.scan_empty_rs_files()
            
            if not empty_files:
                logger.info("没有发现空的rs文件，工作完成")
                break
            
            logger.info(f"发现 {len(empty_files)} 个空文件待处理")
            
            # 首先执行一次cargo build
            success, error_message = self.cargo_build()
            if not success:
                logger.warning("初始cargo build失败，继续处理空文件")
            
            # 处理每个空文件
            for rs_file in empty_files:
                if not self.process_empty_file(rs_file):
                    logger.error(f"处理文件失败: {rs_file}，退出")
                    return 1
        
        logger.info("="*60)
        logger.info("自动化翻译工作流完成！")
        logger.info("="*60)
        return 0


def main():
    """主函数"""
    parser = argparse.ArgumentParser(
        description='自动化C到Rust翻译工作流工具',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
示例:
  %(prog)s --feature myfeature
  %(prog)s --feature myfeature --project-root /path/to/project
        """
    )
    
    parser.add_argument(
        '--feature',
        type=str,
        required=True,
        help='功能名称'
    )
    
    parser.add_argument(
        '--project-root',
        type=str,
        default=None,
        help='项目根目录路径（默认为当前工作目录）'
    )
    
    args = parser.parse_args()
    
    # 创建并运行自动翻译工作流
    workflow = AutoTranslate(args.feature, args.project_root)
    exit_code = workflow.run()
    
    sys.exit(exit_code)


if __name__ == '__main__':
    main()
