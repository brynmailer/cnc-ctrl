use std::{net, time};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::connection::{ActiveConnection, Connection, ConnectionConfig};

pub struct Machine {
    pub config: MachineConfig,
    connection: ActiveConnection<net::TcpStream>,
}

#[derive(Debug, Deserialize)]
pub struct MachineConfig {
    pub connection: ConnectionConfig,
    pub rx_capacity: usize,
}

impl Machine {
    pub fn connect(config: MachineConfig) -> Result<Machine> {
        let device = match config.connection {
            ConnectionConfig::TCP {
                ref address,
                port,
                timeout,
            } => (|| -> Result<net::TcpStream> {
                let stream = net::TcpStream::connect_timeout(
                    &(format!("{}:{}", address, port).parse()?),
                    time::Duration::from_millis(timeout),
                )?;

                stream.set_nonblocking(true)?;

                Ok(stream)
            })()
            .with_context(|| format!("Failed to open TCP connection to {}:{}", address, port))?,
        };

        let connection = Connection::new(device)
            .open()
            .context("Failed to set up connection")?;

        Ok(Self { config, connection })
    }
}
