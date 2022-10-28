use std::{convert::Infallible, fmt::Debug, future::Future};

use async_trait::async_trait;
use http::{header::CONTENT_LENGTH, Request, Response};
use httparse::Header;
use tokio::net::ToSocketAddrs;

use crate::Error;

pub trait FromBody {
    type Error: std::error::Error + Send + Sync + 'static;

    fn from_body(body: &[u8]) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl FromBody for () {
    type Error = Infallible;

    fn from_body(_: &[u8]) -> Result<Self, Self::Error> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FromBodyError(pub String);

impl std::fmt::Display for FromBodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let FromBodyError(msg) = self;

        write!(f, "ERROR: {msg}")
    }
}

impl std::error::Error for FromBodyError {}


pub trait FromRequest {
    type Error: Send + Sync + Debug;

    fn from_request(request: &[u8]) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl FromRequest for String {
    type Error = std::string::FromUtf8Error;

    fn from_request(request: &[u8]) -> Result<Self, Self::Error> {
        String::from_utf8(request.to_vec())
    }
}

impl<T> FromRequest for Request<T>
where
    T: FromBody + Send + Debug,
{
    type Error = Error;

    fn from_request(request: &[u8]) -> Result<Self, Self::Error> {
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut parsed = httparse::Request::new(&mut headers);

        // TODO: handle incomplete request      v
        let body_offset = parsed.parse(request)?.unwrap();

        let body = T::from_body(&request[body_offset..])?;

        let mut req = Request::builder();
        req = req.method(parsed.method.unwrap());
        req = req.uri(parsed.path.unwrap());

        for header in headers.into_iter().filter(|h| !h.name.is_empty()) {
            let Header { name: key, value } = header;

            req = req.header(key, value);
        }

        let req = req.body(body)?;

        Ok(req)
    }
}

pub trait IntoBody {
    fn into_body(self) -> String;
}

impl IntoBody for () {
    fn into_body(self) -> String {
        String::new()
    }
}

impl IntoBody for String {
    fn into_body(self) -> String {
        self
    }
}

pub trait IntoResponse {
    fn into_response(self) -> Vec<u8>;
}

impl IntoResponse for String {
    fn into_response(self) -> Vec<u8> {
        self.into_bytes()
    }
}

impl<T> IntoResponse for Response<T>
where
    T: IntoBody,
{
    fn into_response(self) -> Vec<u8> {
        let (mut parts, body) = self.into_parts();

        let status_str = parts.status.canonical_reason().unwrap_or("UNKNOWN");
        let status = parts.status.as_u16();
        let body = body.into_body() + "\r\n";

        if parts.headers.get(CONTENT_LENGTH).is_none() {
            parts
                .headers
                .insert(CONTENT_LENGTH, body.len().to_string().parse().unwrap());
        }

        let headers = parts.headers.iter().fold(String::new(), |acc, (key, val)| {
            format!("{acc}{}: {}\r\n", key, val.to_str().unwrap())
        });

        format!("HTTP/1.1 {status} {status_str}\r\n{headers}\r\n{body}").into_bytes()
    }
}

#[async_trait]
pub trait Handler<Input, Output>: Send + Sync {
    async fn call(&self, input: Input) -> Output;
}

#[async_trait]
impl<Input, F, Fut, Out, Err> Handler<Input, Result<Out, Err>> for F
where
    Input: Send + 'static,
    Err: Debug,
    Fut: Future<Output = Result<Out, Err>> + Send,
    F: Fn(Input) -> Fut + Send + Sync + 'static,
{
    async fn call(&self, input: Input) -> Result<Out, Err> {
        (self)(input).await
    }
}

#[async_trait]
pub trait Service {
    type Input: Send + 'static;
    type Output: Send + 'static;

    async fn start<H>(
        input: impl ToSocketAddrs + Send,
        output: impl ToSocketAddrs + Send,
        handler: H,
    ) -> Result<(), Error>
    where
        H: Handler<Self::Input, Self::Output> + 'static;
}
