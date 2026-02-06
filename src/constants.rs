//! Constants used throughout the application

/// Maximum number of attempts to translate a file (1 initial + 2 retries)
pub const MAX_TRANSLATION_ATTEMPTS: usize = 3;

/// Maximum number of attempts to fix build errors for a single file
pub const MAX_FIX_ATTEMPTS: usize = 10;

/// Number of lines to preview from code files (C source or Rust code)
pub const CODE_PREVIEW_LINES: usize = 15;

/// Number of lines to preview from error messages
pub const ERROR_PREVIEW_LINES: usize = 10;
