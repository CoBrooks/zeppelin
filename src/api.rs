use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use http::{Response, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::signal::unix::SignalKind;

use crate::Error;
use crate::service::*;

pub struct Api<I, O> {
    __input: PhantomData<I>,
    __output: PhantomData<O>,
}

impl<I, O> Api<I, O>
where
    I: FromRequest + Send + 'static,
    O: IntoResponse + Send + Sync + 'static,
{
    #[instrument(skip_all, level = "debug")]
    async fn handle_connection<H>(stream: TcpStream, handler: Arc<H>) -> Result<Vec<u8>, Error>
    where
        I: Debug,
        O: Debug,
        H: Handler<<Self as Service>::Input, <Self as Service>::Output>
    {
        let (mut reader, mut writer) = stream.into_split();

        let mut buf = [0; 1024];
        let amt = reader.read(&mut buf).await?;

        let input = I::from_request(&buf[..amt]).unwrap();
        debug!(request = ?input);

        match handler.call(input).await {
            Ok(response) => {
                debug!(?response);
                let response = response.into_response();
                writer.write_all(&response).await?;

                Ok(response)
            }
            Err(err) => {
                let msg = format!("ERROR: {err}");
                error!("{err}");

                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(msg)
                    .unwrap()
                    .into_response();

                writer.write_all(&response).await?;

                Err(err)
            }
        }
    }
}

#[async_trait]
impl<I, O> Service for Api<I, O>
where
    I: FromRequest + Send + Debug + 'static,
    O: IntoResponse + Send + Sync + Debug + 'static,
{
    type Input = I;
    type Output = Result<O, Error>;

    async fn start<H>(
        input: impl ToSocketAddrs + Send,
        output: impl ToSocketAddrs + Send,
        handler: H,
    ) -> Result<(), Error>
    where
        H: Handler<Self::Input, Self::Output> + Send + Sync + 'static,
    {
        crate::init_tracing();

        let input = TcpListener::bind(input).await?;
        let output = TcpListener::bind(output).await?;

        let handler = Arc::new(handler);

        let mut subscribers: Vec<OwnedWriteHalf> = Vec::new();

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);

        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;

        info!("API Service started on {}", input.local_addr()?);
        info!("Outputting events on {}", output.local_addr()?);

        loop {
            let tx = tx.clone();

            tokio::select! {
                Ok((stream, _)) = input.accept() => {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Ok(bytes) = Self::handle_connection(stream, handler).await {
                            tx.send(bytes).await.unwrap();
                        } 
                    });
                }

                Ok((stream, addr)) = output.accept() => {
                    let (_reader, writer) = stream.into_split();
                    info!(%addr, "Adding new subscriber");
                    subscribers.push(writer);
                }

                // TODO: async write to outputs so slow subscriber does not
                //       stall new requests
                Some(bytes) = rx.recv() => {
                    for sub in subscribers.iter_mut() {
                        sub.write_all(&bytes).await.unwrap();
                    }
                }

                Some(_) = sigterm.recv() => {
                    error!("Received SIGTERM");
                    break Err("Received SIGTERM".into());
                }
            }
        }
    }
}
