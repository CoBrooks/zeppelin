use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::service::*;

#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromBody for Json<T>
where
    T: for<'de> Deserialize<'de>,
{
    type Error = FromBodyError;

    fn from_body(body: &[u8]) -> Result<Self, Self::Error> {
        let body: T = serde_json::from_slice(body).map_err(|e| FromBodyError(e.to_string()))?;

        Ok(Json(body))
    }
}

impl<T> IntoBody for Json<T>
where
    T: Serialize,
{
    fn into_body(self) -> String {
        serde_json::to_string(&self.0).unwrap()
    }
}

