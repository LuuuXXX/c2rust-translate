//! 应用程序中使用的常量

/// 翻译文件的最大尝试次数（1 次初始 + 2 次重试）
pub const MAX_TRANSLATION_ATTEMPTS: usize = 3;

/// 从代码文件预览的行数（C 源代码或 Rust 代码）
pub const CODE_PREVIEW_LINES: usize = 15;

/// 从错误消息预览的行数
pub const ERROR_PREVIEW_LINES: usize = 10;
