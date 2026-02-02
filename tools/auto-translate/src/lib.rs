pub mod error;
pub mod commands;
pub mod config;
pub mod env;
pub mod file_scanner;
pub mod git;
pub mod compiler;
pub mod translator;

pub use error::{AutoTranslateError, Result};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
