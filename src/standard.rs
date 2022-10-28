use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::signal::unix::SignalKind;

use crate::Error;
use crate::service::*;

pub struct Standard<I, O> {
    __input: PhantomData<I>,
    __output: PhantomData<O>,
}

#[async_trait]
impl<I, O> Service for Standard<I, O>
where
    I: FromRequest + Send + 'static,
    O: IntoResponse + Send + 'static,
{
    type Input = I;
    type Output = Result<O, Error>;

    async fn start<H>(
        input: impl ToSocketAddrs + Send,
        output: impl ToSocketAddrs + Send,
        handler: H,
    ) -> Result<(), Error>
    where
        H: Handler<Self::Input, Self::Output> + 'static,
    {
        let mut input = TcpStream::connect(input).await?;
        let output = TcpListener::bind(output).await?;

        let handler = Arc::new(handler);

        let mut subscribers: Vec<OwnedWriteHalf> = Vec::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;

        let mut buf = [0; 1024];
        loop {
            tokio::select! {
                Ok(amt) = input.read(&mut buf) => {
                    if amt == 0 { break Err("Input connection dropped".into()); }

                    let handler = handler.clone();
                    let tx = tx.clone();

                    tokio::spawn(async move {
                        let i = I::from_request(&buf[..amt]).unwrap();

                        match handler.call(i).await {
                            Ok(result) => {
                                tx.send(result.into_response()).await.unwrap();
                            },
                            Err(e) => eprintln!("ERROR: {e}")
                        };

                        Ok::<_, Infallible>(())
                    });
                },

                Ok((stream, _)) = output.accept() => {
                    let (_reader, writer) = stream.into_split();

                    subscribers.push(writer);
                },

                // TODO: async write to outputs so slow subscriber does not
                //       stall new requests
                Some(bytes) = rx.recv() => {
                    for sub in subscribers.iter_mut() {
                        sub.write_all(&bytes).await.unwrap();
                    }
                }
                
                Some(_) = sigterm.recv() => {
                    break Err("Received SIGTERM".into());
                }
            }
        }
    }
}
