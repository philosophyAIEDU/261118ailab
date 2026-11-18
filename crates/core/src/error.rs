//! 앱 전체에서 사용하는 오류 형식. 화면에 그대로 보여 줄 수 있는 한국어 메시지를 담는다.
//! 연구자료 원문은 오류 메시지에 넣지 않는다 (로그 최소화 원칙).
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    File(String),
    #[error("데이터베이스 오류: {0}")]
    Db(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Ai(String),
    #[error("{0}")]
    Blocked(String),
    #[error("{0}")]
    Backup(String),
    #[error("작업이 취소되었습니다")]
    Cancelled,
}

pub type AppResult<T> = Result<T, AppError>;

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound("요청한 항목을 찾을 수 없습니다".into()),
            other => AppError::Db(other.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        // 경로·파일명은 포함하지 않는다.
        AppError::File(format!("파일을 처리하지 못했습니다 ({})", e.kind()))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Invalid(format!("데이터 형식 오류: {e}"))
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(_: zip::result::ZipError) -> Self {
        AppError::Backup("압축 파일이 손상되었거나 형식이 올바르지 않습니다".into())
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        let kind = match self {
            AppError::File(_) => "file",
            AppError::Db(_) => "db",
            AppError::Unsupported(_) => "unsupported",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Ai(_) => "ai",
            AppError::Blocked(_) => "blocked",
            AppError::Backup(_) => "backup",
            AppError::Cancelled => "cancelled",
        };
        st.serialize_field("kind", kind)?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}
