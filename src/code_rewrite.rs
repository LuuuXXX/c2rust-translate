use crate::translator;
use anyhow::Result;
use colored::Colorize;
use std::path::Path;
use quote::ToTokens;
use syn::visit::Visit;
use syn::visit_mut::VisitMut;

/// Apply error fix to translated file
pub(crate) fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    build_error: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    println!(
        "│ {}",
        "⚠ Build failed, attempting to fix errors..."
            .yellow()
            .bold()
    );
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());

    if try_apply_local_build_error_fix(rs_file, &build_error.to_string())? {
        println!(
            "│ {}",
            "✓ Applied local compiler-error fix".bright_green()
        );
        return Ok(());
    }

    // Fix translation error
    // Always show full fix code, but respect user preference for error preview
    translator::fix_translation_error(
        feature,
        file_type,
        rs_file,
        &build_error.to_string(),
        show_full_output, // User preference for error preview
        true,             // Always show full fix code
    )?;

    // Verify fix produced output
    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }

    println!("│ {}", "✓ Fix applied".bright_green());

    Ok(())
}

fn try_apply_local_build_error_fix(rs_file: &Path, build_error: &str) -> Result<bool> {
    if try_fix_static_mut_array_pointer_access(rs_file, build_error)? {
        return Ok(true);
    }
    if try_fix_c_string_slice_pointer_cast(rs_file, build_error)? {
        return Ok(true);
    }
    if try_fix_option_fn_unwrap_mismatch(rs_file, build_error)? {
        return Ok(true);
    }
    if try_wrap_unsafe_call_from_e0133(rs_file, build_error)? {
        return Ok(true);
    }
    Ok(false)
}

#[derive(Default)]
struct UnsafeExprCounter {
    count: usize,
}

impl Visit<'_> for UnsafeExprCounter {
    fn visit_expr_unsafe(&mut self, node: &syn::ExprUnsafe) {
        self.count += 1;
        syn::visit::visit_expr_unsafe(self, node);
    }
}

struct UnsafeRegionCollapser;

impl VisitMut for UnsafeRegionCollapser {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, expr);
        if let syn::Expr::Unsafe(expr_unsafe) = expr {
            let block = expr_unsafe.block.clone();
            *expr = syn::Expr::Block(syn::ExprBlock {
                attrs: expr_unsafe.attrs.clone(),
                label: None,
                block,
            });
        }
    }
}

fn has_export_name_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.meta.to_token_stream().to_string().contains("export_name"))
}

fn should_collapse_fn_unsafe_regions(item_fn: &syn::ItemFn) -> bool {
    if item_fn.sig.unsafety.is_some() {
        return false;
    }
    if item_fn.sig.abi.is_none() || !has_export_name_attr(&item_fn.attrs) {
        return false;
    }
    let mut counter = UnsafeExprCounter::default();
    counter.visit_block(&item_fn.block);
    counter.count >= 2
}

fn collapse_fn_unsafe_regions(item_fn: &mut syn::ItemFn) -> bool {
    if !should_collapse_fn_unsafe_regions(item_fn) {
        return false;
    }

    let mut collapser = UnsafeRegionCollapser;
    collapser.visit_block_mut(&mut item_fn.block);

    let old_block = item_fn.block.as_ref().clone();
    item_fn.block = Box::new(syn::Block {
        brace_token: old_block.brace_token,
        stmts: vec![syn::Stmt::Expr(
            syn::Expr::Unsafe(syn::ExprUnsafe {
                attrs: Vec::new(),
                unsafe_token: Default::default(),
                block: old_block,
            }),
            None,
        )],
    });
    true
}

pub(crate) fn try_collapse_exported_function_unsafe_regions(rs_file: &Path) -> Result<bool> {
    let source = std::fs::read_to_string(rs_file)?;
    let mut ast = match syn::parse_file(&source) {
        Ok(ast) => ast,
        Err(_) => return Ok(false),
    };

    let mut changed = false;
    for item in &mut ast.items {
        if let syn::Item::Fn(item_fn) = item {
            changed |= collapse_fn_unsafe_regions(item_fn);
        }
    }

    if !changed {
        return Ok(false);
    }

    let mut rendered = prettyplease::unparse(&ast);
    if source.ends_with('\n') {
        rendered.push('\n');
    }
    std::fs::write(rs_file, rendered)?;
    Ok(true)
}

pub(crate) fn try_normalize_c_char_literal_ptrs(rs_file: &Path) -> Result<bool> {
    let source = std::fs::read_to_string(rs_file)?;
    let re = regex::Regex::new(
        r#"b"((?:\\.|[^"\\])*)\\0"\.as_ptr\(\)\s+as\s+\*const\s+::core::ffi::c_char"#,
    )?;
    let updated = re.replace_all(&source, r#"c"$1".as_ptr()"#).into_owned();
    if updated == source {
        return Ok(false);
    }
    std::fs::write(rs_file, updated)?;
    Ok(true)
}

fn try_fix_static_mut_array_pointer_access(rs_file: &Path, build_error: &str) -> Result<bool> {
    let mentions_static_mut_refs = build_error.contains("static_mut_refs")
        || build_error.contains("creating a mutable reference to mutable static")
        || build_error.contains("creating a shared reference to mutable static");
    let mentions_array_ptr_get = build_error.contains("array_ptr_get");
    if !mentions_static_mut_refs && !mentions_array_ptr_get {
        return Ok(false);
    }

    let source = std::fs::read_to_string(rs_file)?;
    let mut updated = source.clone();

    let ptr_patterns = [
        (
            regex::Regex::new(r"\(&raw mut\s+([A-Za-z_][A-Za-z0-9_]*)\)\.as_mut_ptr\(\)")?,
            "::core::ptr::addr_of_mut!($1).cast()",
        ),
        (
            regex::Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\.as_mut_ptr\(\)")?,
            "::core::ptr::addr_of_mut!($1).cast()",
        ),
    ];

    if mentions_static_mut_refs || mentions_array_ptr_get {
        for (pattern, replacement) in ptr_patterns {
            updated = pattern.replace_all(&updated, replacement).into_owned();
        }
    }

    let array_len = regex::Regex::new(r"\*const\s+\[[^;\]]+;\s*([0-9_]+)\]")?
        .captures(build_error)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));

    if let Some(array_len) = array_len {
        let size_patterns = [
            regex::Regex::new(r"core::mem::size_of_val\(&([A-Za-z_][A-Za-z0-9_]*)\)")?,
            regex::Regex::new(r"\(&raw const\s+([A-Za-z_][A-Za-z0-9_]*)\)\.len\(\)")?,
            regex::Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\.len\(\)")?,
        ];
        for pattern in size_patterns {
            updated = pattern.replace_all(&updated, array_len.as_str()).into_owned();
        }
    }

    if updated == source {
        return Ok(false);
    }

    std::fs::write(rs_file, updated)?;
    Ok(true)
}

fn try_fix_c_string_slice_pointer_cast(rs_file: &Path, build_error: &str) -> Result<bool> {
    if !build_error.contains("slice_ptr_get") && !build_error.contains("casting `&*const") {
        return Ok(false);
    }

    let source = std::fs::read_to_string(rs_file)?;
    let pattern = regex::Regex::new(
        r#"unsafe\s*\{\s*&(?P<lit>c"((?:\\.|[^"\\])*)"\.as_ptr\(\))\s+as\s+\*const\s+\[::core::ffi::c_char\]\s*\}\s*\.as_ptr\(\)"#,
    )?;
    let updated = pattern.replace_all(&source, "$lit").into_owned();

    if updated == source {
        return Ok(false);
    }

    std::fs::write(rs_file, updated)?;
    Ok(true)
}

fn try_fix_option_fn_unwrap_mismatch(rs_file: &Path, build_error: &str) -> Result<bool> {
    if !build_error.contains("expected enum `Option<")
        || !build_error.contains("found fn pointer")
    {
        return Ok(false);
    }

    let source = std::fs::read_to_string(rs_file)?;
    let pattern = regex::Regex::new(
        r#"(?P<prefix>[A-Za-z_][A-Za-z0-9_\[\]\.\(\)\s]*?)\.func\.unwrap\(\)"#,
    )?;
    let updated = pattern.replace_all(&source, "${prefix}.func").into_owned();

    if updated == source {
        return Ok(false);
    }

    std::fs::write(rs_file, updated)?;
    Ok(true)
}

fn try_wrap_unsafe_call_from_e0133(rs_file: &Path, build_error: &str) -> Result<bool> {
    if !build_error.contains("error[E0133]: call to unsafe function") {
        return Ok(false);
    }

    let source = std::fs::read_to_string(rs_file)?;
    let mut lines: Vec<String> = source.lines().map(|line| line.to_string()).collect();
    let rel_path = rs_file
        .strip_prefix(crate::util::find_project_root()?)
        .ok()
        .and_then(|path| path.to_str())
        .map(|s| s.replace('\\', "/"));

    let location_re = regex::Regex::new(r"--> ([^:]+):(\d+):(\d+)")?;
    let assign_re = regex::Regex::new(
        r"^(?P<indent>\s*)(?P<prefix>(?:let\s+[^=]+=\s*|return\s+)?)(?P<call>[A-Za-z_][A-Za-z0-9_]*\s*\([^;]*\))(?P<suffix>\s*;.*)$",
    )?;

    let mut changed = false;
    for captures in location_re.captures_iter(build_error) {
        let Some(path) = captures.get(1).map(|m| m.as_str().replace('\\', "/")) else {
            continue;
        };
        let Some(expected_path) = &rel_path else {
            continue;
        };
        if !path.ends_with(expected_path) {
            continue;
        }

        let Ok(line_no) = captures[2].parse::<usize>() else {
            continue;
        };
        if line_no == 0 || line_no > lines.len() {
            continue;
        }

        let line = lines[line_no - 1].clone();
        if line.contains("unsafe {") {
            continue;
        }

        if let Some(m) = assign_re.captures(&line) {
            let indent = m.name("indent").map(|m| m.as_str()).unwrap_or("");
            let prefix = m.name("prefix").map(|m| m.as_str()).unwrap_or("");
            let call = m.name("call").map(|m| m.as_str()).unwrap_or("");
            let suffix = m.name("suffix").map(|m| m.as_str()).unwrap_or("");
            lines[line_no - 1] = format!("{indent}{prefix}unsafe {{ {call} }}{suffix}");
            changed = true;
        }
    }

    if !changed {
        return Ok(false);
    }

    let mut updated = lines.join("\n");
    if source.ends_with('\n') {
        updated.push('\n');
    }
    std::fs::write(rs_file, updated)?;
    Ok(true)
}

/// Apply warning fix to translated file
///
/// Similar to `apply_error_fix` but used during Phase 2 (warning fixing).
/// The build has not failed -- warnings were surfaced by running without `-A warnings`.
pub(crate) fn apply_warning_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    warning_msg: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    println!(
        "│ {}",
        "⚠ Warnings detected, attempting to fix...".yellow().bold()
    );
    println!("│");
    println!(
        "│ {}",
        format_progress("Warning Fix").bright_magenta().bold()
    );

    // Fix using the same translation tool, passing warnings as the "error" message
    translator::fix_translation_error(
        feature,
        file_type,
        rs_file,
        &warning_msg.to_string(),
        show_full_output,
        true,
    )?;

    // Verify fix produced output
    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Warning fix failed: output file is empty");
    }

    println!("│ {}", "✓ Warning fix applied".bright_green());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collapse_exported_fn_unsafe_regions_rewrites_multi_unsafe_body() {
        let source = r#"
use super::*;
#[unsafe(export_name = "XSUM_benchInternal")]
pub extern "C" fn XSUM_benchInternal(key_size: usize) -> ::core::ffi::c_int {
    let buffer = unsafe { calloc(key_size + 19, 1) };
    if buffer.is_null() {
        unsafe { exit(12) };
    }
    let aligned = unsafe {
        let ptr = (buffer as *mut u8).add(15);
        ptr as *const ::core::ffi::c_void
    };
    unsafe { XSUM_benchMem(aligned, key_size) };
    unsafe { free(buffer) };
    0
}
"#;
        let mut file = syn::parse_file(source).unwrap();
        let syn::Item::Fn(item_fn) = &mut file.items[1] else {
            panic!("expected fn item");
        };

        assert!(collapse_fn_unsafe_regions(item_fn));
        let rendered = prettyplease::unparse(&file);

        assert!(rendered.contains("unsafe {"));
        assert_eq!(rendered.matches("unsafe {").count(), 1);
        assert!(rendered.contains("XSUM_benchInternal"));
        assert!(!rendered.contains("let buffer = unsafe"));
        assert!(!rendered.contains("unsafe { calloc"));
        assert!(!rendered.contains("unsafe { exit"));
        assert!(rendered.contains("XSUM_benchMem"));
        assert!(rendered.contains("free(buffer)"));
    }

    #[test]
    fn test_collapse_exported_fn_unsafe_regions_skips_single_unsafe_call() {
        let source = r#"
use super::*;
#[unsafe(export_name = "XSUM_autox86")]
pub extern "C" fn XSUM_autox86() -> *const ::core::ffi::c_char {
    let vec_version: ::core::ffi::c_int = unsafe { XXH_featureTest() };
    match vec_version {
        0 => b"scalar\0".as_ptr() as *const ::core::ffi::c_char,
        _ => b"avx\0".as_ptr() as *const ::core::ffi::c_char,
    }
}
"#;
        let mut file = syn::parse_file(source).unwrap();
        let syn::Item::Fn(item_fn) = &mut file.items[1] else {
            panic!("expected fn item");
        };

        assert!(!collapse_fn_unsafe_regions(item_fn));
        let rendered = prettyplease::unparse(&file);
        assert_eq!(rendered.matches("unsafe {").count(), 1);
        assert!(rendered.contains("let vec_version: ::core::ffi::c_int = unsafe { XXH_featureTest() };"));
    }

    #[test]
    fn test_fix_static_mut_array_pointer_access_rewrites_array_pointer_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let rs_file = dir.path().join("fun_test.rs");
        std::fs::write(
            &rs_file,
            r#"use super::*;
pub unsafe extern "C" fn test() {
    XSUM_fillTestBuffer((&raw mut g_benchSecretBuf).as_mut_ptr(), core::mem::size_of_val(&g_benchSecretBuf));
}"#,
        )
        .unwrap();

        let build_error = r#"error[E0658]: use of unstable library feature `array_ptr_get`
  --> src/fun_test.rs:2:37
   |
2  |     (&raw mut g_benchSecretBuf).as_mut_ptr(),
   |                                 ^^^^^^^^^^
error[E0599]: no method named `len` found for raw pointer `*const [u8; 136]` in the current scope"#;

        assert!(try_fix_static_mut_array_pointer_access(&rs_file, build_error).unwrap());
        let updated = std::fs::read_to_string(&rs_file).unwrap();
        assert!(updated.contains("::core::ptr::addr_of_mut!(g_benchSecretBuf).cast()"));
        assert!(updated.contains("136"));
        assert!(!updated.contains("as_mut_ptr()"));
        assert!(!updated.contains("size_of_val(&g_benchSecretBuf)"));
    }

    #[test]
    fn test_fix_static_mut_array_pointer_access_rewrites_len_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let rs_file = dir.path().join("fun_test.rs");
        std::fs::write(
            &rs_file,
            r#"use super::*;
pub unsafe extern "C" fn test() {
    XSUM_fillTestBuffer(g_benchSecretBuf.as_mut_ptr(), g_benchSecretBuf.len());
}"#,
        )
        .unwrap();

        let build_error = r#"error: creating a mutable reference to mutable static
error: creating a shared reference to mutable static
error[E0599]: no method named `len` found for raw pointer `*const [u8; 136]` in the current scope"#;

        assert!(try_fix_static_mut_array_pointer_access(&rs_file, build_error).unwrap());
        let updated = std::fs::read_to_string(&rs_file).unwrap();
        assert!(updated.contains("::core::ptr::addr_of_mut!(g_benchSecretBuf).cast()"));
        assert!(updated.contains(", 136)"));
    }

    #[test]
    fn test_fix_c_string_slice_pointer_cast_simplifies_weird_assert_arg() {
        let dir = tempfile::tempdir().unwrap();
        let rs_file = dir.path().join("fun_test.rs");
        std::fs::write(
            &rs_file,
            r#"use super::*;
pub unsafe extern "C" fn test() {
    __assert_fail(c"a".as_ptr(), c"b".as_ptr(), 1, unsafe { &c"foo".as_ptr() as *const [::core::ffi::c_char] }.as_ptr());
}"#,
        )
        .unwrap();

        let build_error = "error[E0658]: use of unstable library feature `slice_ptr_get`";
        assert!(try_fix_c_string_slice_pointer_cast(&rs_file, build_error).unwrap());
        let updated = std::fs::read_to_string(&rs_file).unwrap();
        assert!(updated.contains("c\"foo\".as_ptr()"));
        assert!(!updated.contains("*const [::core::ffi::c_char]"));
    }

    #[test]
    fn test_fix_option_fn_unwrap_mismatch_removes_unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let rs_file = dir.path().join("fun_test.rs");
        std::fs::write(
            &rs_file,
            r#"use super::*;
pub unsafe extern "C" fn test(hashFuncID: usize) {
    XSUM_benchHash(g_hashesToBench[hashFuncID].func.unwrap(), c"name".as_ptr(), 1, ::core::ptr::null(), 0);
}"#,
        )
        .unwrap();

        let build_error = r#"error[E0308]: mismatched types
expected enum `Option<unsafe extern "C" fn(*const c_void, usize, u32) -> u32>`
found fn pointer"#;
        assert!(try_fix_option_fn_unwrap_mismatch(&rs_file, build_error).unwrap());
        let updated = std::fs::read_to_string(&rs_file).unwrap();
        assert!(updated.contains("g_hashesToBench[hashFuncID].func,"));
        assert!(!updated.contains(".unwrap()"));
    }
}
