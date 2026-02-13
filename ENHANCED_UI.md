# 增强的用户交互界面

本文档描述了在 c2rust-translate 工具中实现的增强用户交互功能。

## 概述

该工具现在提供了更直观和灵活的用户交互体验，包括：
- 并排代码比较显示
- 基于上下文的交互式提示
- 用于批处理的自动接受模式
- 在所有交互场景中保持一致的用户界面

## 新功能

### 1. 代码比较显示

该工具现在以格式化的比较视图并排显示 C 和 Rust 代码：

```
═══════════════════════════════════════════════════════════════════
                   C vs Rust 代码比较                        
═══════════════════════════════════════════════════════════════════
┌─────── C 源代码 ────────┬─────── Rust 代码 ────────────────┐
│ 1 int add(int a, int b) {    │ 1 pub fn add(a: i32, b: i32)    │
│ 2     return a + b;          │ 2     -> i32 {                  │
│ 3 }                          │ 3     a + b                      │
│                              │ 4 }                              │
└──────────────────────────────┴──────────────────────────────────┘

═══════════════════════════════════════════════════════════════════
                         测试结果                                
═══════════════════════════════════════════════════════════════════
✓ 所有测试通过
```

### 2. 基于上下文的交互式提示

#### 场景 1：编译成功且测试通过

当编译成功且所有测试通过时，您会看到一个交互式选择菜单：

```
│
│ ✓ Compilation and tests successful!
│
? What would you like to do?
❯ Accept this code (will be committed)
  Auto-accept all subsequent translations
  Manual fix (edit the file with VIM)
  Exit (abort the translation process)
```

**使用方法**：
- 使用 **↑↓ 方向键**或 **j/k**（Vim 模式）在选项间移动
- 当前选择用 **❯** 符号高亮显示
- 按 **回车键** 确认选择

**选项说明**：

**选项 1 - Accept**：接受当前翻译并提交。

**选项 2 - Auto-accept**：为当前会话启用自动接受模式，自动接受所有未来的成功翻译而无需提示。对批处理很有用。

**选项 3 - Manual fix**：在 VIM 中打开 Rust 文件进行手动编辑，然后重新构建和测试。

**选项 4 - Exit**：中止翻译过程。

#### 场景 2：测试失败

当编译成功但测试失败时，您会看到一个交互式选择菜单：

```
│
│ ⚠ Tests failed - What would you like to do?
│
? Select an option:
❯ Retry directly (without adding suggestion)
  Add fix suggestion for AI to modify
  Manual fix (edit the file with VIM)
  Exit (abort the translation process)
```

**使用方法**：
- 使用 **↑↓ 方向键**或 **j/k**（Vim 模式）在选项间移动
- 按 **回车键** 确认选择

**选项说明**：

**选项 1 - Retry directly**：直接重试而不添加修复建议。

**选项 2 - Add suggestion**：提示您输入修复建议，AI 将使用该建议修改代码。

**选项 3 - Manual fix**：在 VIM 中打开文件进行手动编辑。

**选项 4 - Exit**：中止翻译过程。

#### 场景 3：编译失败（达到最大重试次数）

当达到最大修复尝试次数后编译仍然失败时，您会看到一个交互式选择菜单：

```
│
│ ⚠ Compilation failed - What would you like to do?
│
? Select an option:
❯ Retry directly (without adding suggestion)
  Add fix suggestion for AI to modify
  Manual fix (edit the file with VIM)
  Exit (abort the translation process)
```

**使用方法**：与场景 2 相同，使用方向键或 Vim 键（j/k）导航，回车确认。

这些选项的工作方式与测试失败场景相同。

### 3. 文件选择交互增强

工具现在提供了更直观的**多选交互界面**来选择要处理的文件：

#### 旧方式（文本输入）
```
Available files to process:
  1. src/var_counter.rs
  2. src/fun_calculate.rs
  3. src/var_global.rs
  4. src/fun_helper.rs

Select files to process:
  - Enter numbers separated by commas (e.g., 1,3,5)
  - Enter ranges (e.g., 1-3,5)
  - Enter 'all' to process all files

Your selection: _
```
用户需要手动输入数字、范围或逗号分隔的列表。

#### 新方式（交互式多选）
```

Available files to process:

Use arrow keys to navigate, Space to select/deselect, Enter to confirm
Press 'a' to select all files

? Select files to process:
 ◯ src/var_counter.rs
❯◉ src/fun_calculate.rs
 ◯ src/var_global.rs
 ◉ src/fun_helper.rs
```

**使用方法**：
- 使用 **↑↓ 方向键**或 **j/k**（Vim 模式）在文件间移动
- 按 **空格键** 选择/取消选择当前文件
- 按 **a 键** 快速选择所有文件
- 按 **回车键** 确认选择
- **◯** 表示未选择，**◉** 表示已选择
- **❯** 表示当前光标位置

**优势**：
- 更直观的视觉反馈
- 无需记忆复杂的输入格式
- 减少输入错误
- 快速批量选择

### 4. 自动接受模式

自动接受模式允许您在无需手动干预的情况下处理多个文件：

- 通过在测试通过时选择 "Auto-accept all subsequent translations" 启用
- 启用后，所有未来的成功翻译都会自动被接受
- 对批量处理大型代码库特别有用
- 模式基于会话（重启工具时会重置）

### 5. 改进的错误上下文

所有交互式提示现在都会显示：
- 文件位置（C 源文件和 Rust 目标文件）
- 并排代码比较
- 构建或测试错误消息
- 清晰的结果指示器（✓ 表示成功，✗ 表示失败）

## 实现细节

### 依赖库

- **`inquire` v0.7**：提供交互式命令行界面
  - `Select`：单选菜单组件
  - `MultiSelect`：多选菜单组件
  - 支持 Vim 模式导航
  - 无安全漏洞（已通过 GitHub Advisory Database 检查）

### 新模块

- **`src/diff_display.rs`**：处理并排代码比较显示
- 增强的 **`src/interaction.rs`**：使用 `inquire` 库提供交互式选择界面
- 增强的 **`src/file_scanner.rs`**：使用 `MultiSelect` 提供文件多选界面

### 新枚举

```rust
// 用于编译成功且测试通过的情况
pub enum CompileSuccessChoice {
    Accept,
    AutoAccept,
    ManualFix,
    Exit,
}

// 用于测试或编译失败的情况
pub enum FailureChoice {
    RetryDirectly,
    AddSuggestion,
    ManualFix,
    Exit,
}
```

### 关键函数

**交互式选择函数（使用 inquire）**：
- `prompt_compile_success_choice()`：测试成功时的 4 选项交互式选择
- `prompt_test_failure_choice()`：测试失败时的 4 选项交互式选择
- `prompt_compile_failure_choice()`：编译失败时的 4 选项交互式选择
- `prompt_build_failure_choice()`：构建失败时的 4 选项交互式选择
- `prompt_user_choice()`：通用的 3 选项交互式选择
- `prompt_file_selection()`：文件多选交互界面

**其他功能函数**：
- `display_code_comparison()`：并排代码显示
- `is_auto_accept_mode()`, `enable_auto_accept_mode()`：自动接受模式管理
- `prompt_suggestion()`：修复建议文本输入（保持文本输入，因为需要自由输入）

## 向后兼容性

增强的用户界面保持向后兼容性：
- 保留现有的 `UserChoice` 枚举以实现兼容性
- 所有公共 API 保持不变
- 所有新功能都是内部实现的改进，不影响外部接口
- 文件选择仍支持 `--allow-all` 标志跳过交互

## 测试

运行测试套件以验证实现：

```bash
cargo test
```

所有 60+ 个单元测试都通过，确保：
- 枚举变体正常工作
- 自动接受模式状态管理正常运作
- 代码比较显示处理边缘情况
- 文件路径显示正常工作

