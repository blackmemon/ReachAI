//! One error type for the services. The gateway maps it to an HTTP status and a problem body.

use query_engine::QueryError;
use shared::ApiError;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Error {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest(String),
    Fields(BTreeMap<String, String>),
    Conflict(String),
    TooMany,
    Unavailable(String),
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn status(&self) -> u16 {
        match self {
            Error::Unauthorized => 401,
            Error::Forbidden => 403,
            Error::NotFound => 404,
            Error::BadRequest(_) | Error::Fields(_) => 400,
            Error::Conflict(_) => 409,
            Error::TooMany => 429,
            Error::Unavailable(_) => 503,
            Error::Internal(_) => 500,
        }
    }

    pub fn body(&self) -> ApiError {
        match self {
            Error::Unauthorized => ApiError::new("unauthorized", "Please sign in."),
            Error::Forbidden => {
                ApiError::new("forbidden", "You do not have permission to do this.")
            }
            Error::NotFound => ApiError::new("not_found", "This record was not found."),
            Error::BadRequest(m) => ApiError::new("bad_request", m),
            Error::Fields(f) => {
                let mut e = ApiError::new("invalid", "Please check the highlighted fields.");
                e.fields = f.clone();
                e
            }
            Error::Conflict(m) => ApiError::new("conflict", m),
            Error::TooMany => {
                ApiError::new("too_many", "Too many tries. Please wait a few minutes.")
            }
            Error::Unavailable(m) => ApiError::new("unavailable", m),
            // Never show internal details to the user.
            Error::Internal(_) => ApiError::new(
                "internal",
                "Something went wrong on our side. Please try again.",
            ),
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Internal(e.to_string())
    }
}

impl From<QueryError> for Error {
    fn from(e: QueryError) -> Self {
        match e {
            QueryError::BadRequest(m) => Error::BadRequest(m),
            QueryError::Fields(f) => Error::Fields(f),
            QueryError::NotFound => Error::NotFound,
            QueryError::Conflict => Error::Conflict(
                "This record was changed by someone else. Reload it and try again.".into(),
            ),
            QueryError::Duplicate(c) => {
                let msg = if c.contains("email") {
                    "A person with this email already exists."
                } else if c.contains("domain") {
                    "A company with this website already exists."
                } else {
                    "This record already exists."
                };
                Error::Conflict(msg.into())
            }
            QueryError::Db(e) => Error::Internal(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_hide_details() {
        let e = Error::Internal("password=secret at line 1".into());
        assert_eq!(e.status(), 500);
        assert!(!e.body().message.contains("secret"));
    }

    #[test]
    fn statuses() {
        assert_eq!(Error::Unauthorized.status(), 401);
        assert_eq!(Error::Forbidden.status(), 403);
        assert_eq!(Error::TooMany.status(), 429);
        assert_eq!(Error::Unavailable("offline".into()).status(), 503);
        assert_eq!(Error::Conflict("x".into()).status(), 409);
    }

    #[test]
    fn duplicate_email_message() {
        let e: Error = QueryError::Duplicate("contacts_email_unique".into()).into();
        assert!(matches!(&e, Error::Conflict(m) if m.contains("email")));
    }
}
