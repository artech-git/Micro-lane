use std::fmt::Display;

use thiserror::Error;


pub type Error = Box<dyn std::error::Error>;
pub type BackendResult<T> = std::result::Result<T, Error>;
pub type ProgramResult<T> = std::result::Result<T, Box<dyn std::process::Termination>>;


#[derive(Error, Debug)]
pub enum IOErrors { 
    
    // errors originating from tokio io operations
    #[error("tokio io error: {0}")]
    TokioIOError(#[from] tokio::io::Error),

    // errors originating from undefined operations
    #[error("undefined error: {0}")]
    UndefinedErrors(#[from] Box<dyn std::error::Error>),

}
