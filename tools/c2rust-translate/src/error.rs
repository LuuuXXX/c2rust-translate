use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutoTranslateError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("Project root (.c2rust directory) not found")]
    ProjectRootNotFound,
    
    #[error("Translation failed: {0}")]
    TranslationFailed(String),
    
    #[error("Compilation failed after {0} attempts")]
    CompilationFailed(usize),
    
    #[error("Build command failed: {0}")]
    BuildFailed(String),
    
    #[error("Test command failed: {0}")]
    TestFailed(String),
    
    #[error("Git operation failed: {0}")]
    GitFailed(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Command execution failed: {0}")]
    CommandFailed(String),
    
    #[error("File scanning error: {0}")]
    FileScanError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, AutoTranslateError>;
