use rmcp::{
    RoleServer,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::Transport,
};
use std::{io, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

/// SDK receive currently reads an uncapped line; this boundary never buffers beyond the limit.
pub struct Stdio {
    input: BufReader<Stdin>,
    output: Arc<Mutex<Stdout>>,
    pending: Vec<u8>,
    limit: usize,
    stop: CancellationToken,
}
impl Stdio {
    pub fn new(limit: usize, stop: CancellationToken) -> Self {
        Self {
            input: BufReader::new(tokio::io::stdin()),
            output: Arc::new(Mutex::new(tokio::io::stdout())),
            pending: Vec::new(),
            limit,
            stop,
        }
    }
}
impl Transport<RoleServer> for Stdio {
    type Error = io::Error;
    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let output = self.output.clone();
        async move {
            let mut bytes = serde_json::to_vec(&item)
                .map_err(|_| io::Error::other("Cannot encode protocol response"))?;
            bytes.push(b'\n');
            tokio::time::timeout(Duration::from_secs(30), async {
                let mut output = output.lock().await;
                output.write_all(&bytes).await?;
                output.flush().await
            })
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "Protocol output deadline exceeded")
            })?
        }
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            let available = tokio::select! {_=self.stop.cancelled()=>return None,result=self.input.fill_buf()=>match result{Ok(bytes)=>bytes,Err(_)=>{self.stop.cancel();return None;}}};
            if available.is_empty() {
                self.stop.cancel();
                return None;
            }
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |n| n + 1);
            if self
                .pending
                .len()
                .checked_add(count)
                .is_none_or(|n| n > self.limit)
            {
                self.stop.cancel();
                return None;
            }
            self.pending.extend_from_slice(&available[..count]);
            let complete = available[count - 1] == b'\n';
            self.input.consume(count);
            if !complete {
                continue;
            }
            let parsed = serde_json::from_slice(&self.pending);
            self.pending.clear();
            match parsed {
                Ok(message) => return Some(message),
                Err(_) => {
                    self.stop.cancel();
                    return None;
                }
            }
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        self.stop.cancel();
        self.pending.clear();
        tokio::time::timeout(Duration::from_secs(5), self.output.lock().await.shutdown())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Protocol shutdown deadline exceeded",
                )
            })?
    }
}
