use std::convert::Infallible;
use std::fmt::Debug;
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
    I: FromRequest + Debug + Send + 'static,
    O: IntoResponse + Debug + Send + 'static,
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
        crate::init_tracing();

        let mut input = TcpStream::connect(input).await?;
        debug!("Receiving events on {}", input.local_addr()?);

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
                        debug!(request = ?i);

                        match handler.call(i).await {
                            Ok(result) => {
                                debug!(?result);
                                tx.send(result.into_response()).await.unwrap();
                            },
                            Err(err) => error!("{err}")
                        };

                        Ok::<_, Infallible>(())
                    });
                },

                Ok((stream, addr)) = output.accept() => {
                    let (_reader, writer) = stream.into_split();
                    info!(%addr, "Adding new subscriber");
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
                    error!("Received SIGTERM");
                    break Err("Received SIGTERM".into());
                }
            }
        }
    }
}
