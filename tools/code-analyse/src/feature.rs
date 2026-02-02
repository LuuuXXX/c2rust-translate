use crate::{get_root, File, Kind, Result, ToError};
use hierr::Error;
use higuard::Guard;
use syn::{Item, UseTree, Visibility};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

pub struct Feature {
    root: PathBuf,
    prefix: PathBuf,
    name: String,
    files: Vec<File>,
}

impl Feature {
    pub fn new(name: &str) -> Result<Self> {
        let root = get_root()?.join(".c2rust").join(name);
        let prefix = Self::get_file_prefix(&root.join("c"));
        let mut this = Self {
            root,
            name: name.to_string(),
            prefix,
            files: vec![],
        };
        this.get_files()?;
        Ok(this)
    }

    fn get_files(&mut self) -> Result<()> {
        let c_root = self.root.join("c");
        for entry in WalkDir::new(&c_root)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() && path.extension() != Some(OsStr::new("c2rust")) {
                continue;
            }
            self.files.push(File::new(&c_root, path)?);
        }
        Ok(())
    }

    fn get_file_prefix(c_root: &Path) -> PathBuf {
        let mut prefix = c_root.to_path_buf();
        while let Some(child) = Self::get_single_subdir(&prefix) {
            prefix = child;
        }
        prefix
    }

    fn get_single_subdir(path: &Path) -> Option<PathBuf> {
        let mut child = None;
        for entry in WalkDir::new(path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if child.is_some() || !path.is_dir() {
                return None;
            }
            child = Some(path.to_path_buf());
        }
        child
    }

    // 检查节点是否为需要处理的定义（函数或变量定义）
    fn is_node_definition(node: &crate::Node) -> bool {
        match node.kind {
            crate::Kind::FunctionDecl(_) => !node.inner.is_empty(),
            crate::Kind::VarDecl(_) => !node.kind.is_extern() || node.kind.is_inited(),
            _ => false,
        }
    }

    ///
    /// 如果全部初始化成功才删除已经存在的内容.
    ///
    pub fn reinit(&self) -> Result<()> {
        println!("Starting reinitialization for feature '{}'", self.name);
        let rust = self.root.join("rust");
        let _ = fs::rename(&rust, self.root.join("rust_old"));
        println!("Backed up existing rust directory to rust_old");
        let guard = Guard::make(|_| {
            let _ = fs::remove_dir_all(&rust);
            let _ = fs::rename(self.root.join("rust_old"), &rust);
        });
        println!("Creating new Rust library project...");
        let output = Command::new("cargo")
            .current_dir(&self.root)
            .arg("new")
            .arg("--lib")
            .arg("rust")
            .output()
            .log_err("cargo new failed")?;

        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            return Err(Error::general());
        }
        println!("Rust project created successfully");
        println!("Setting crate type to cdylib...");
        self.set_cdylib()?;
        println!("Crate type configured");
        println!("Creating file directory structure...");
        self.create_file_directories()?;
        println!("Directory structure created");
        println!("Generating type information with bindgen...");
        self.generate_types()?;
        println!("Type information generated");
        guard.discard();
        let _ = fs::remove_dir_all(self.root.join("rust_old"));
        println!("Cleaned up backup directory");
        println!("Feature '{}' reinitialized successfully", self.name);
        Ok(())
    }

    /// 检查每个变量和函数翻译状态和对应的rust文件内容是否一致
    /// 如果不一致(已翻译但文件为空或者未翻译但是文件非空), 则根据rust文件内容更新真实的翻译状态,
    /// 更新后需要重新生成C文件.
    pub fn update(&mut self) -> Result<()> {
        println!("Starting update for feature '{}'", self.name);
        let mut changed = false;
        let prefix = &self.prefix;
        let c_root = self.root.join("c");

        for file in &mut self.files {
            // 获取File对应的mod目录名
            let mod_name = Self::get_mod_name_for_file(prefix, file)?;
            let mod_dir = self.root.join("rust/src").join(&mod_name);

            // 如果mod目录不存在，说明还没有对应的Rust文件，跳过
            if !mod_dir.exists() {
                continue;
            }

            // 收集需要更新的节点及其新状态
            let mut updates: Vec<(String, bool)> = Vec::new();

            for node in file.iter() {
                // 只处理函数和变量定义
                if !Self::is_node_definition(node) {
                    continue;
                }

                // 获取节点名
                let Some(name) = node.kind.name() else {
                    continue;
                };

                // 构建对应的Rust文件路径
                let rust_file_path = mod_dir.join(name).with_extension("rs");

                // 检查文件内容是否为空（去除空白字符后）
                let is_file_empty = Self::is_file_empty(&rust_file_path)?;

                let has_committed = node.kind.has_committed();

                // 检查状态是否一致
                if has_committed && is_file_empty {
                    // 已标记为翻译但文件为空，需要重置状态为未翻译
                    updates.push((name.to_string(), false));
                } else if !has_committed && !is_file_empty {
                    // 未标记为翻译但文件非空，需要标记为已翻译
                    updates.push((name.to_string(), true));
                }
                // 其他情况（状态一致）不需要处理
            }

            // 应用更新到file的节点
            if !updates.is_empty() {
                changed = true;
                println!("Updating {} nodes in file: {}", updates.len(), mod_name);
                for (name, committed) in updates {
                    // 在file中找到对应的node并更新状态
                    for node in file.iter_mut() {
                        if let Some(node_name) = node.kind.name() {
                            if node_name == name {
                                node.kind.set_git_commit(committed);
                                break;
                            }
                        }
                    }
                }
                // 保存file的JSON状态
                file.save_json()?;
                file.export_c_code(&c_root)?;
                println!("Saved JSON state and exported C code for file: {}", mod_name);
            }
        }

        if changed {
            println!("Feature '{}' updated with changes", self.name);
        } else {
            println!("Feature '{}' already up to date", self.name);
        }
        Ok(())
    }

    /// Rust文件和C源码文件一一对应，即合并原来同一个文件下独立的多个变量和函数的Rust文件
    /// 合并时主要检查两点:
    /// 1. 依赖的外表FFI声明去重
    /// 1. 根据名字恢复C代码中符号可见性
    /// 1. 合并后需要编译，可能存在因为符号冲突导致的编译错误，需要手工修正.
    pub fn merge(&mut self) -> Result<()> {
        println!("Starting merge for feature '{}'", self.name);
        let mut any_merged = false;

        for file in &self.files {
            println!("Processing file for merge: {}", file.path().display());
            if self.merge_file(file)? {
                any_merged = true;
                println!("File merged successfully: {}", file.path().display());
                self.cargo_build()?;
            }
        }

        if any_merged {
            println!("Feature '{}' merged successfully", self.name);
        } else {
            println!("Feature '{}': no files needed merging", self.name);
        }
        Ok(())
    }

    fn set_cdylib(&self) -> Result<()> {
        // TODO: 后续通过toml文件操作API实现读写
        // 当前简单追加写实现
        let mut file = fs::File::options()
            .append(true)
            .open(self.root.join("rust/Cargo.toml"))
            .to_err()?;
        file.write(b"\n[lib]\ncrate-type = [\"cdylib\"]\n").to_err()?;
        Ok(())
    }

    fn create_file_directories(&self) -> Result<()> {
        let mut mod_items = "// generated by c2rust\n\nmod types;\nuse types::*;\n\n".to_string(); 
        for file in &self.files {
            let mod_name = self.create_file_mod(file)?;
            mod_items.push_str("mod ");
            mod_items.push_str(&mod_name);
            mod_items.push_str(";\n");
        }
        fs::write(self.root.join("rust/src/lib.rs"), mod_items.as_bytes()).to_err()?;
        Ok(())
    }

    fn create_file_mod(&self, file: &File) -> Result<String> {
        let file_path = file.path();
        let rel_path = file_path
            .strip_prefix(&self.prefix)
            .unwrap()
            .with_extension("");
        let mod_name = rel_path.display().to_string().replace("/", "_");
        let mod_dir = self.root.join("rust/src").join(&mod_name);
        fs::create_dir_all(&mod_dir).to_err()?;

        let mut nodes = HashMap::new();
        for node in file.iter() {
            if !Self::is_node_definition(node) {
                continue;
            }
            let Some(name) = node.kind.name() else {
                continue;
            };
            // 对于变量，如果有初始化则需要提取其代码否则任何一个都可以.
            nodes
                .entry(name)
                .and_modify(|old| {
                    if node.kind.is_inited() {
                        *old = node;
                    }
                })
                .or_insert(node);
        }

        let c_root = self.root.join("c");
        let mut mod_items = "// generated by c2rust\n\n".to_string();
        for (name, node) in nodes {
            mod_items.push_str("mod ");
            mod_items.push_str(name);
            mod_items.push_str(";\n");
            fs::File::create(mod_dir.join(name).with_extension("rs")).to_err()?;
            let c_code = node.kind.c_code(&c_root, node.inner.is_empty())?;
            fs::write(mod_dir.join(name).with_extension("c"), c_code.as_bytes()).to_err()?;
        }
        fs::write(mod_dir.join("mod.rs"), mod_items.as_bytes()).to_err()?;
        Ok(mod_name)
    }

    pub fn generate_types(&self) -> Result<()> {
        self.generate_types_h()?;
        self.generate_types_rs()?;
        Ok(())
    }

    pub fn generate_types_h(&self) -> Result<()> {
        let types_h = self.root.join("rust/src/types.h");
        let mut content = String::new();
        let mut seen_names = HashSet::new();
        let mut forward_names = HashSet::new();
        let mut typedef_names = HashSet::new();
        let c_root = self.root.join("c");
        for file in &self.files {
            for node in file.iter() {
                let id = node
                    .kind
                    .name()
                    .map(|s| s.to_string())
                    .or(node.kind.line_info())
                    .unwrap();
                let is_type = match node.kind {
                    Kind::RecordDecl(_) | Kind::EnumDecl(_) => {
                        if node.kind.is_definition() {
                            seen_names.insert(id)
                        } else {
                            forward_names.insert(id)
                        }
                    }
                    Kind::TypedefDecl(_) => typedef_names.insert(id),
                    Kind::VarDecl(_) => seen_names.insert(id),
                    _ => false,
                };
                if !is_type {
                    continue;
                }
                let code = node.kind.c_code(&c_root, node.inner.is_empty())?;
                content.push_str(&code);
                content.push('\n');
            }
        }
        fs::write(&types_h, content.as_bytes()).to_err()?;
        Ok(())
    }

    pub fn generate_types_rs(&self) -> Result<()> {
        let output = Command::new("bindgen")
            .current_dir(&self.root)
            .arg("rust/src/types.h")
            .arg("-o")
            .arg("rust/src/types.rs")
            .arg("--no-layout-tests")
            .arg("--default-enum-stype")
            .arg("consts")
            .arg("--disable-nested-struct-naming")
            .output()
            .to_err()?;
        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            return Err(Error::last());
        }
        Ok(())
    }

    // 获取File对应的mod目录名
    fn get_mod_name_for_file(prefix: &Path, file: &File) -> Result<String> {
        let file_path = file.path();
        let rel_path = file_path
            .strip_prefix(prefix)
            .map_err(|_| Error::general())?
            .with_extension("");
        Ok(rel_path.display().to_string().replace("/", "_"))
    }

    // 合并单个File对应的Rust文件
    fn merge_file(&self, file: &File) -> Result<bool> {
        use syn::{Abi, LitStr, token::{Brace, Extern, Unsafe}};

        // 获取File对应的mod目录名
        let mod_name = Self::get_mod_name_for_file(&self.prefix, file)?;
        let mod_dir = self.root.join("rust/src").join(&mod_name);

        // 如果mod目录不存在，说明还没有Rust文件，跳过
        if !mod_dir.exists() {
            return Ok(false);
        }

        // 收集目录下的所有.rs文件（排除mod.rs）
        let rs_files = Self::collect_rust_files(&mod_dir)?;

        // 如果没有rs文件，跳过
        if rs_files.is_empty() {
            return Ok(false);
        }

        println!("Merging {} .rs files for mod {}", rs_files.len(), mod_name);

        // 解析所有rs文件，合并内容
        let (all_items, defined_funcs) = Self::parse_rust_files(&rs_files)?;

        // 重新处理items，过滤和合并FFI声明和use语句
        let (foreign_mods, use_items, other_items) = Self::group_items_by_type(all_items);

        // 合并所有extern块
        let mut merged_foreign_mod: Option<Item> = None;
        if !foreign_mods.is_empty() {
            let mut all_ffi_items = Vec::new();
            let mut first_mod_attrs: Option<Vec<syn::Attribute>> = None;
            let mut first_mod_unsafety: Option<Unsafe> = None;
            let mut first_mod_abi: Option<Abi> = None;

            for foreign_mod in foreign_mods {
                if first_mod_attrs.is_none() {
                    first_mod_attrs = Some(foreign_mod.attrs.clone());
                    first_mod_unsafety = foreign_mod.unsafety;
                    first_mod_abi = Some(foreign_mod.abi);
                }

                // 过滤掉已定义函数的FFI声明
                for ffi_item in foreign_mod.items {
                    if let syn::ForeignItem::Fn(ffi_fn) = &ffi_item {
                        if defined_funcs.contains(&ffi_fn.sig.ident.to_string()) {
                            continue; // 跳过已定义函数的FFI声明
                        }
                    }
                    all_ffi_items.push(ffi_item);
                }
            }

            // 去重FFI项（基于函数名）
            let mut unique_ffi_items = Vec::new();
            let mut seen_names = HashSet::new();

            for ffi_item in all_ffi_items {
                // 提取函数名
                let name = if let syn::ForeignItem::Fn(ffi_fn) = &ffi_item {
                    ffi_fn.sig.ident.to_string()
                } else {
                    // 对于非函数项，保留原样
                    continue;
                };

                // 根据函数名去重
                if !seen_names.contains(&name) {
                    seen_names.insert(name);
                    unique_ffi_items.push(ffi_item);
                }
            }

            // 创建合并的extern块，使用第一个块的属性
            if !unique_ffi_items.is_empty() {
                // 如果没有abi，使用默认的extern "C"
                let abi = first_mod_abi.unwrap_or_else(|| syn::Abi {
                    extern_token: Extern::default(),
                    name: Some(LitStr::new("C", proc_macro2::Span::call_site())),
                });

                let foreign_mod = syn::ItemForeignMod {
                    attrs: first_mod_attrs.unwrap_or_default(),
                    unsafety: first_mod_unsafety,
                    abi,
                    brace_token: Brace::default(),
                    items: unique_ffi_items,
                };
                merged_foreign_mod = Some(Item::ForeignMod(foreign_mod));
            }
        }

        // 处理use语句的去重（基于UseTree内容的精细化去重）
        let unique_use_items = Self::deduplicate_use_items(use_items);

        // 构建最终的items向量，按正确顺序：use语句、extern块、其他item
        let mut final_items = Vec::new();

        // 添加use语句
        for use_item in unique_use_items {
            final_items.push(Item::Use(use_item));
        }

        // 添加合并的extern块（如果有）
        if let Some(foreign_mod) = merged_foreign_mod {
            final_items.push(foreign_mod);
        }

        // 添加其他item
        final_items.extend(other_items);

        // 处理_c2rust_private_前缀的可见性
        self.adjust_visibility_for_private_symbols(&mut final_items);

        // 创建合并后的AST
        let merged_ast = syn::File {
            shebang: None,
            attrs: Vec::new(),
            items: final_items,
        };

        // 生成合并后的代码
        let merged_code = quote::quote!(#merged_ast).to_string();

        // 写入合并文件：rust/src/<mod_name>.rs
        let merged_file_path = self.root.join("rust/src").join(&mod_name).with_extension("rs");
        fs::write(&merged_file_path, merged_code).to_err()?;

        // 删除原目录
        fs::remove_dir_all(&mod_dir).to_err()?;
        println!("Deleted directory: {}", mod_dir.display());

        Ok(true)
    }

    // 调整_c2rust_private_前缀符号的可见性
    fn adjust_visibility_for_private_symbols(&self, items: &mut [syn::Item]) {
        for item in items.iter_mut() {
            match item {
                Item::Fn(item_fn) => {
                    let ident_str = item_fn.sig.ident.to_string();
                    if ident_str.starts_with("_c2rust_private_") {
                        // 确保没有pub可见性
                        item_fn.vis = Visibility::Inherited;
                    }
                }
                Item::Static(item_static) => {
                    let ident_str = item_static.ident.to_string();
                    if ident_str.starts_with("_c2rust_private_") {
                        // 确保没有pub可见性
                        item_static.vis = Visibility::Inherited;
                    }
                }
                _ => {}
            }
        }
    }

    // 从UseTree中收集所有导入项字符串
    fn collect_import_strings(tree: &UseTree) -> Vec<String> {
        let mut imports = Vec::new();
        let mut stack = vec![(tree, Vec::new())];

        while let Some((node, mut current_path)) = stack.pop() {
            match node {
                UseTree::Path(path) => {
                    current_path.push(path.ident.to_string());
                    stack.push((&path.tree, current_path));
                }
                UseTree::Name(name) => {
                    current_path.push(name.ident.to_string());
                    imports.push(current_path.join("::"));
                }
                UseTree::Rename(rename) => {
                    current_path.push(rename.rename.to_string());
                    imports.push(current_path.join("::"));
                    // 注意：忽略别名，因为去重只关心导入的项
                }
                UseTree::Glob(_) => {
                    imports.push(format!("{}::*", current_path.join("::")));
                }
                UseTree::Group(group) => {
                    for item in &group.items {
                        stack.push((item, current_path.clone()));
                    }
                }
            }
        }
        imports
    }

    // 检查一个导入项是否已被覆盖（考虑通配符）
    fn import_is_covered(import: &str, seen_imports: &HashSet<String>) -> bool {
        // 完全匹配
        if seen_imports.contains(import) {
            return true;
        }
        // 如果是具体导入，检查是否存在对应的通配符导入
        if !import.ends_with("::*") {
            // 将路径分段，检查每一级父路径是否有通配符
            let segments: Vec<&str> = import.split("::").collect();
            for i in 1..segments.len() {
                let wildcard = segments[0..i].join("::") + "::*";
                if seen_imports.contains(&wildcard) {
                    return true;
                }
            }
        }
        // 如果是通配符导入，检查是否存在相同的通配符
        // 注意：我们不考虑具体导入覆盖通配符的情况，因为可能没有导入所有项
        false
    }

    // 收集目录下的所有.rs文件（排除mod.rs）
    fn collect_rust_files(mod_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut rs_files = Vec::new();
        for entry in fs::read_dir(mod_dir).to_err()? {
            let entry = entry.to_err()?;
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false) {
                let file_name = path.file_name().unwrap().to_string_lossy();
                if file_name == "mod.rs" {
                    continue; // 跳过mod.rs文件
                }
                rs_files.push(path);
            }
        }
        Ok(rs_files)
    }

    // 解析所有Rust文件，收集items和已定义的函数名
    fn parse_rust_files(rs_files: &[PathBuf]) -> Result<(Vec<syn::Item>, HashSet<String>)> {
        let mut all_items = Vec::new();
        let mut defined_funcs = HashSet::new();

        for rs_file in rs_files {
            let content = fs::read_to_string(rs_file).to_err()?;
            let ast = syn::parse_file(&content).map_err(|e| {
                eprintln!("Failed to parse {}: {}", rs_file.display(), e);
                Error::inval()
            })?;

            for item in ast.items {
                if let syn::Item::Fn(item_fn) = &item {
                    defined_funcs.insert(item_fn.sig.ident.to_string());
                }
                all_items.push(item);
            }
        }

        Ok((all_items, defined_funcs))
    }

    // 将items按类型分组：extern块、use语句、其他items
    fn group_items_by_type(items: Vec<syn::Item>) -> (Vec<syn::ItemForeignMod>, Vec<syn::ItemUse>, Vec<syn::Item>) {
        let mut foreign_mods = Vec::new();
        let mut use_items = Vec::new();
        let mut other_items = Vec::new();

        for item in items {
            match item {
                syn::Item::ForeignMod(foreign_mod) => foreign_mods.push(foreign_mod),
                syn::Item::Use(use_item) => use_items.push(use_item),
                _ => other_items.push(item),
            }
        }

        (foreign_mods, use_items, other_items)
    }

    // 对use语句进行去重（基于导入内容的精细化去重）
    fn deduplicate_use_items(use_items: Vec<syn::ItemUse>) -> Vec<syn::ItemUse> {
        let mut unique_use_items = Vec::new();
        let mut seen_imports = HashSet::new();

        for use_item in use_items {
            let imports = Self::collect_import_strings(&use_item.tree);
            let all_covered = imports.iter().all(|import| Self::import_is_covered(import, &seen_imports));

            if !all_covered {
                unique_use_items.push(use_item);
                for import in imports {
                    seen_imports.insert(import);
                }
            }
        }

        unique_use_items
    }

    // 执行cargo build检查编译
    fn cargo_build(&self) -> Result<()> {
        println!("Running cargo build to verify compilation...");
        let output = Command::new("cargo")
            .current_dir(self.root.join("rust"))
            .arg("build")
            .output()
            .to_err()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Cargo build failed:\n{}", stderr);
            return Err(Error::general());
        }

        println!("Cargo build succeeded!");
        Ok(())
    }


    // 检查文件是否为空（去除空白字符后）
    fn is_file_empty(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(true);
        }
        let content = fs::read_to_string(path).to_err()?;
        Ok(content.trim().is_empty())
    }
}
