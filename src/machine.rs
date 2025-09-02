use std::{net, time};

use anyhow::{Context, Result};

use crate::config::{ConnectionConfig, MachineConfig};
use crate::connection::{ActiveConnection, Connection};

pub struct Machine<'a> {
    pub config: &'a MachineConfig,
    connection: ActiveConnection<net::TcpStream>,
}

impl<'a> Machine<'a> {
    pub fn connect(config: &'a MachineConfig) -> Result<Machine> {
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
            .with_context(|| format!("Failed to create TCP connection to {}:{}", address, port))?,
        };

        let connection = Connection::new(device)
            .open()
            .context("Failed to set up connection")?;

        Ok(Self { config, connection })
    }
}
