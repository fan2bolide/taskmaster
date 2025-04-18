#[cfg(test)]
mod test_utils;

mod commands_impl;
mod error;

use std::sync::{Arc, Mutex as StdMutex};

use crate::task_manager::TaskManagerTrait;

pub use error::Error;
use error::Result;

use commands::{ClientCommand, ServerCommand};
use connection::Connection;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex as TokioMutex,
};

pub struct ClientHandler<Stream, TaskManager> {
    client_id: u64,
    task_manager: Arc<TokioMutex<TaskManager>>,
    connection: Connection<Stream, ServerCommand, ClientCommand>,
}

impl<Stream, TaskManager> ClientHandler<Stream, TaskManager>
where
    Stream: AsyncWrite + AsyncRead + Unpin,
    TaskManager: TaskManagerTrait,
{
    pub async fn process_client(
        socket: Stream,
        task_manager: Arc<TokioMutex<TaskManager>>,
    ) -> Result<()> {
        let mut handler = Self::new(socket, task_manager)?;

        handler
            .write_frame(&ClientCommand::SuccessfulConnection)
            .await?;

        handler.handle_loop().await
    }

    fn new(socket: Stream, task_manager: Arc<TokioMutex<TaskManager>>) -> Result<Self> {
        static NEXT_CLIENT_ID: StdMutex<u64> = StdMutex::new(0);

        let client_id = {
            let mut lock = NEXT_CLIENT_ID
                .lock()
                .expect("ClientHandler::new() mutex is poisoned");
            let next_client_id = *lock;
            *lock += 1;
            next_client_id
        };

        let handler = Self {
            client_id,
            task_manager,
            connection: Connection::new(socket, 4096),
        };

        eprintln!("Client {} has connected", handler.client_id);
        Ok(handler)
    }

    async fn handle_loop(mut self) -> Result<()> {
        while let Some(command) = self.read_frame().await? {
            match command {
                ServerCommand::ListTasks => self.handle_list_tasks().await?,
            }
        }
        Ok(())
    }

    async fn read_frame(&mut self) -> Result<Option<ServerCommand>> {
        match self.connection.read_frame().await {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = self.write_frame(&ClientCommand::FailedToParseFrame).await;
                Err(Error::FailedToReadFrameFromClient {
                    client_id: self.client_id,
                    error,
                })
            }
        }
    }

    async fn write_frame(&mut self, frame: &ClientCommand) -> Result<()> {
        match self.connection.write_frame(frame).await {
            Ok(value) => Ok(value),
            Err(error) => Err(Error::FailedToWriteFrameFromClient {
                client_id: self.client_id,
                error,
            }),
        }
    }
}

impl<Stream, TaskManager> Drop for ClientHandler<Stream, TaskManager> {
    fn drop(&mut self) {
        eprintln!("Client {} has disconnected", self.client_id);
    }
}
