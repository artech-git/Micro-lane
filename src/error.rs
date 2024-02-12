

pub type Error = Box<dyn std::error::Error>;
pub type BackendResult<T> = std::result::Result<T, Error>;
