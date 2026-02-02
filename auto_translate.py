#!/usr/bin/env python3
"""
C to Rust Automated Translation and Hybrid Build Tool

This script automates the process of translating C code to Rust and performing
hybrid builds with the translated code.
"""

import argparse
import os
import sys
import subprocess
import glob
from pathlib import Path
import toml


def log_info(message):
    """Print info message"""
    print(f"[INFO] {message}")


def log_error(message):
    """Print error message"""
    print(f"[ERROR] {message}", file=sys.stderr)


def log_warning(message):
    """Print warning message"""
    print(f"[WARNING] {message}")


def run_command(cmd, cwd=None, check=True, env=None, capture_output=True):
    """
    Run a shell command and return the result
    
    Args:
        cmd: Command to run (string or list)
        cwd: Working directory
        check: Whether to check return code
        env: Environment variables
        capture_output: Whether to capture output
    
    Returns:
        CompletedProcess object
    """
    if isinstance(cmd, str):
        shell = True
    else:
        shell = False
    
    log_info(f"Running command: {cmd}")
    
    try:
        if capture_output:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                check=check,
                shell=shell,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True
            )
        else:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                check=check,
                shell=shell,
                env=env,
                text=True
            )
        return result
    except subprocess.CalledProcessError as e:
        if check:
            log_error(f"Command failed with exit code {e.returncode}")
            if capture_output:
                log_error(f"STDOUT: {e.stdout}")
                log_error(f"STDERR: {e.stderr}")
            raise
        return e


def git_commit(message, cwd=None):
    """
    Commit all changes in the repository
    
    Args:
        message: Commit message
        cwd: Working directory
    """
    # Check if there are changes to commit
    result = run_command("git status --porcelain", cwd=cwd)
    if result.stdout.strip():
        log_info(f"Committing changes: {message}")
        run_command("git add .", cwd=cwd)
        run_command(f'git commit -m "{message}"', cwd=cwd)
    else:
        log_info("No changes to commit")


def check_tool_exists(tool_name):
    """
    Check if a tool exists in the system PATH
    
    Args:
        tool_name: Name of the tool to check
    
    Returns:
        True if tool exists, False otherwise
    """
    result = run_command(f"which {tool_name}", check=False)
    return result.returncode == 0


def is_file_empty(file_path):
    """
    Check if a file is empty (only whitespace)
    
    Args:
        file_path: Path to the file
    
    Returns:
        True if file is empty or only contains whitespace
    """
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read().strip()
            return len(content) == 0
    except Exception as e:
        log_error(f"Failed to read file {file_path}: {e}")
        return False


def get_file_type_from_name(filename):
    """
    Extract file type from filename prefix
    
    Args:
        filename: Name of the file
    
    Returns:
        'var' for var_ prefix, 'fn' for fun_ prefix, None otherwise
    """
    if filename.startswith('var_'):
        return 'var'
    elif filename.startswith('fun_'):
        return 'fn'
    else:
        return None


def find_empty_rs_files(rust_dir):
    """
    Find all empty .rs files in the rust directory
    
    Args:
        rust_dir: Path to the rust directory
    
    Returns:
        List of paths to empty .rs files
    """
    empty_files = []
    rs_files = glob.glob(os.path.join(rust_dir, "**/*.rs"), recursive=True)
    
    for rs_file in rs_files:
        if is_file_empty(rs_file):
            empty_files.append(rs_file)
    
    log_info(f"Found {len(empty_files)} empty .rs files")
    return empty_files


def get_corresponding_c_file(rs_file_path):
    """
    Get the corresponding C file path for a Rust file
    
    Args:
        rs_file_path: Path to the .rs file
    
    Returns:
        Path to the corresponding .c file
    """
    # Replace .rs extension with .c
    c_file_path = rs_file_path.rsplit('.rs', 1)[0] + '.c'
    return c_file_path


def translate_file(config_path, file_type, c_file_path, output_path, error_file=None):
    """
    Translate a C file to Rust using translate_and_fix.py
    
    Args:
        config_path: Path to config.toml
        file_type: Type of translation ('var', 'fn', or 'fix')
        c_file_path: Path to input C file (or .rs file for 'fix' type)
        output_path: Path to output .rs file
        error_file: Path to error message file (only for 'fix' type)
    
    Returns:
        True if successful, False otherwise
    """
    cmd = [
        sys.executable,
        "tools/translate_and_fix/translate_and_fix.py",
        "--config", config_path,
        "--type", file_type,
        "--code", c_file_path,
        "--output", output_path
    ]
    
    if error_file and file_type == 'fix':
        cmd.extend(["--error", error_file])
    
    try:
        result = run_command(cmd, check=True)
        return True
    except subprocess.CalledProcessError:
        return False


def cargo_build(rust_dir):
    """
    Run cargo build in the rust directory
    
    Args:
        rust_dir: Path to the rust directory
    
    Returns:
        Tuple of (success: bool, error_output: str)
    """
    try:
        result = run_command("cargo build", cwd=rust_dir, check=True)
        return (True, "")
    except subprocess.CalledProcessError as e:
        error_output = e.stderr if e.stderr else e.stdout
        return (False, error_output)


def extract_error_files_from_cargo_output(error_output):
    """
    Extract file paths that have errors from cargo build output
    
    Args:
        error_output: Cargo build error output
    
    Returns:
        List of file paths with errors
    """
    error_files = []
    lines = error_output.split('\n')
    
    for line in lines:
        # Look for lines like "error[E0XXX]: ..." or "--> src/file.rs:line:col"
        if '-->' in line:
            parts = line.split('-->')
            if len(parts) > 1:
                file_info = parts[1].strip().split(':')[0]
                if file_info not in error_files:
                    error_files.append(file_info)
    
    return error_files


def load_config(config_path):
    """
    Load configuration from config.toml
    
    Args:
        config_path: Path to config.toml
    
    Returns:
        Dictionary with configuration
    """
    try:
        with open(config_path, 'r', encoding='utf-8') as f:
            config = toml.load(f)
        return config
    except Exception as e:
        log_error(f"Failed to load config from {config_path}: {e}")
        return None


def get_build_commands_from_config(config, feature_name):
    """
    Get build commands from configuration
    
    Args:
        config: Configuration dictionary
        feature_name: Feature name
    
    Returns:
        Dictionary with clean, build, and test commands
    """
    feature_key = f"feature.{feature_name}"
    if feature_key not in config:
        # Try default feature
        feature_key = "feature.default"
    
    if feature_key not in config:
        return None
    
    feature_config = config[feature_key]
    
    return {
        'clean': feature_config.get('clean', ''),
        'clean_dir': feature_config.get('clean_dir', ''),
        'build': feature_config.get('build', ''),
        'build_dir': feature_config.get('build_dir', ''),
        'test': feature_config.get('test', ''),
        'test_dir': feature_config.get('test_dir', '')
    }


def main():
    """Main function"""
    parser = argparse.ArgumentParser(
        description='Automated C to Rust translation and hybrid build tool'
    )
    parser.add_argument(
        '--feature',
        type=str,
        required=True,
        help='Feature name to process'
    )
    parser.add_argument(
        '--project-root',
        type=str,
        default='.',
        help='Project root directory (where .c2rust is located)'
    )
    
    args = parser.parse_args()
    feature_name = args.feature
    project_root = os.path.abspath(args.project_root)
    
    log_info(f"Starting automated translation for feature: {feature_name}")
    log_info(f"Project root: {project_root}")
    
    # Paths
    feature_dir = os.path.join(project_root, feature_name)
    rust_dir = os.path.join(feature_dir, "rust")
    config_path = os.path.join(project_root, "tools/translate_and_fix/config.toml")
    
    # Step 1: Initialization - Check if rust directory exists
    log_info("Step 1: Checking if rust directory exists...")
    
    if not os.path.exists(rust_dir):
        log_info(f"Rust directory not found at {rust_dir}")
        log_info("Running code-analyse --init...")
        
        # Check if code-analyse exists
        if not check_tool_exists("code-analyse"):
            log_error("code-analyse tool not found. Please install it first.")
            return 1
        
        # Run code-analyse --init
        try:
            run_command(f"code-analyse --feature {feature_name} --init", cwd=project_root)
        except subprocess.CalledProcessError:
            log_error("code-analyse --init failed")
            return 1
        
        # Check if rust directory was created
        if not os.path.exists(rust_dir):
            log_error(f"code-analyse --init succeeded but {rust_dir} was not created")
            return 1
        
        # Commit the initialization
        git_commit(f"Initialize feature {feature_name} with code-analyse", cwd=project_root)
    else:
        log_info(f"Rust directory exists at {rust_dir}")
    
    # Step 2: Main loop - Process empty .rs files
    log_info("Step 2: Entering main processing loop...")
    
    # Step 2.1: Initial cargo build
    log_info("Step 2.1: Running initial cargo build...")
    success, error_output = cargo_build(rust_dir)
    
    if not success:
        log_warning("Initial cargo build failed. This is expected if there are empty .rs files.")
        log_info("Will proceed with translation...")
    
    # Main loop
    while True:
        # Step 2.2: Find empty .rs files
        log_info("Step 2.2: Scanning for empty .rs files...")
        empty_rs_files = find_empty_rs_files(rust_dir)
        
        if not empty_rs_files:
            log_info("No empty .rs files found. Processing complete!")
            break
        
        # Process each empty .rs file
        for rs_file in empty_rs_files:
            log_info(f"Processing file: {rs_file}")
            
            # Step 2.2.1: Extract file type
            filename = os.path.basename(rs_file)
            file_type = get_file_type_from_name(filename)
            
            if not file_type:
                log_warning(f"Cannot determine file type for {filename} (expected var_ or fun_ prefix)")
                continue
            
            log_info(f"File type: {file_type}")
            
            # Step 2.2.2: Verify corresponding C file exists
            c_file = get_corresponding_c_file(rs_file)
            
            if not os.path.exists(c_file):
                log_error(f"Corresponding C file not found: {c_file}")
                log_error("The project may be corrupted.")
                
                response = input("Do you want to run 'code-analyse --init' to regenerate? (y/n): ")
                if response.lower() == 'y':
                    try:
                        run_command(f"code-analyse --feature {feature_name} --init", cwd=project_root)
                        git_commit(f"Reinitialize feature {feature_name}", cwd=project_root)
                        continue
                    except subprocess.CalledProcessError:
                        log_error("code-analyse --init failed")
                        return 1
                else:
                    log_info("User chose not to reinitialize. Exiting.")
                    return 1
            
            # Step 2.2.3: Call translation tool
            log_info(f"Step 2.2.3: Translating {c_file} to {rs_file}...")
            
            if not translate_file(config_path, file_type, c_file, rs_file):
                log_error(f"Translation failed for {c_file}")
                return 1
            
            # Step 2.2.4: Verify translation result
            if is_file_empty(rs_file):
                log_error(f"Translation succeeded but {rs_file} is still empty")
                return 1
            
            log_info("Translation successful!")
            
            # Step 2.2.5: Compilation error handling loop
            log_info("Step 2.2.5: Building and fixing compilation errors...")
            
            max_fix_iterations = 10
            fix_iteration = 0
            
            while fix_iteration < max_fix_iterations:
                success, error_output = cargo_build(rust_dir)
                
                if success:
                    log_info("Cargo build successful!")
                    break
                
                log_warning(f"Cargo build failed (iteration {fix_iteration + 1}/{max_fix_iterations})")
                
                # Check if the error is related to the current file
                error_files = extract_error_files_from_cargo_output(error_output)
                
                # Write error to temporary file
                error_file_path = f"/tmp/cargo_error_{fix_iteration}.txt"
                with open(error_file_path, 'w', encoding='utf-8') as f:
                    f.write(error_output)
                
                log_info(f"Attempting to fix errors in {rs_file}...")
                
                # Step 2.2.6: Call fix tool
                if not translate_file(config_path, 'fix', rs_file, rs_file, error_file_path):
                    log_error(f"Fix failed for {rs_file}")
                    return 1
                
                # Verify fix result
                if is_file_empty(rs_file):
                    log_error(f"Fix succeeded but {rs_file} is now empty")
                    return 1
                
                fix_iteration += 1
            
            if fix_iteration >= max_fix_iterations:
                log_error(f"Max fix iterations ({max_fix_iterations}) reached. Build still failing.")
                return 1
            
            # Step 2.2.7: Commit translation result
            log_info("Step 2.2.7: Committing translation result...")
            git_commit(f"Translate {filename} from C to Rust", cwd=project_root)
            
            # Step 2.2.8: Update code analysis
            log_info("Step 2.2.8: Updating code analysis...")
            
            try:
                run_command(f"code-analyse --feature {feature_name} --update", cwd=project_root)
            except subprocess.CalledProcessError:
                log_error("code-analyse --update failed")
                return 1
            
            # Step 2.2.9: Commit update
            log_info("Step 2.2.9: Committing code analysis update...")
            git_commit(f"Update code analysis after translating {filename}", cwd=project_root)
            
            # Step 2.2.10 & 2.2.11: Execute build commands with hybrid build
            log_info("Step 2.2.10-11: Executing build/test commands...")
            
            # Load config to get build commands
            config = load_config(config_path)
            if not config:
                log_error("Failed to load configuration")
                return 1
            
            build_cmds = get_build_commands_from_config(config, feature_name)
            if not build_cmds:
                log_warning(f"No build commands found for feature {feature_name}")
            else:
                # Get hybrid build library path (this should be configured)
                # For now, we'll skip this if the library is not available
                hybrid_lib = os.environ.get('HYBRID_BUILD_LIB', '')
                
                # Set up environment for hybrid build
                env = os.environ.copy()
                if hybrid_lib:
                    env['LD_PRELOAD'] = hybrid_lib
                env['C2RUST_FEATURE_ROOT'] = feature_dir
                
                # Execute clean
                if build_cmds.get('clean'):
                    clean_dir = os.path.join(project_root, build_cmds.get('clean_dir', ''))
                    log_info(f"Running clean: {build_cmds['clean']}")
                    try:
                        run_command(build_cmds['clean'], cwd=clean_dir, env=env, capture_output=False)
                    except subprocess.CalledProcessError:
                        log_error("Clean command failed. Please handle manually.")
                        return 1
                
                # Execute build
                if build_cmds.get('build'):
                    build_dir = os.path.join(project_root, build_cmds.get('build_dir', ''))
                    log_info(f"Running build: {build_cmds['build']}")
                    try:
                        run_command(build_cmds['build'], cwd=build_dir, env=env, capture_output=False)
                    except subprocess.CalledProcessError:
                        log_error("Build command failed. Please handle manually.")
                        return 1
                
                # Execute test
                if build_cmds.get('test'):
                    test_dir = os.path.join(project_root, build_cmds.get('test_dir', ''))
                    log_info(f"Running test: {build_cmds['test']}")
                    try:
                        run_command(build_cmds['test'], cwd=test_dir, env=env, capture_output=False)
                    except subprocess.CalledProcessError:
                        log_error("Test command failed. Please handle manually.")
                        return 1
            
            log_info(f"Successfully processed {filename}")
        
        # Step 2.2.12: Loop to process next empty .rs file
        log_info("Step 2.2.12: Continuing to next iteration...")
    
    log_info("All empty .rs files have been processed successfully!")
    return 0


if __name__ == "__main__":
    sys.exit(main())
