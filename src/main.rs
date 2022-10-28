use glide::{
    api::Api,
    formats::Json,
    service::Service,
    standard::Standard,
    Error
};
use http::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = Standard::start("localhost:3000", "localhost:3001", handler);

    Api::start("localhost:8000", "localhost:3000", api_handler).await
}

async fn handler(input: String) -> Result<String, Error> {
    println!("Handling input `{input}`");

    Ok(input.to_uppercase())
}

#[derive(Debug, Deserialize, Serialize)]
struct FooBar {
    foo: String,
}

async fn api_handler(request: Request<Json<FooBar>>) -> Result<Response<Json<FooBar>>, Error> {
    let body = FooBar {
        foo: request.body().foo.to_uppercase(),
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Json(body))?;

    Ok(response)
}

