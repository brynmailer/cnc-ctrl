use std::io::{BufRead, Write};
use std::sync::atomic;
use std::{fs, io, path, process, sync};

use anyhow::{Context, Result, anyhow, bail};
use crossbeam::channel;
use log::{error, info};

use crate::config::{
    OutputConfig, OutputKind, ProcessConfig, StreamConfig, TaskConfig, TaskKind, apply_template,
    expand_path,
};
use crate::connection::message::{Feedback, Push};
use crate::connection::{ActiveConnection, Command, Message, Response};

pub trait Task {
    fn execute(
        &self,
        timestamp: &str,
        running: sync::Arc<atomic::AtomicBool>,
        connection: &ActiveConnection,
    ) -> Result<()>;
}

struct Stream<'a> {
    config: &'a StreamConfig,
}

struct Process<'a> {
    config: &'a ProcessConfig,
}

impl<'a> From<&'a TaskConfig> for Box<dyn Task + 'a> {
    fn from(config: &'a TaskConfig) -> Self {
        match &config.kind {
            TaskKind::Stream(stream_config) => Box::new(Stream {
                config: stream_config,
            }),
            TaskKind::Process(process_config) => Box::new(Process {
                config: process_config,
            }),
        }
    }
}

impl<'a> Task for Stream<'a> {
    fn execute(
        &self,
        timestamp: &str,
        running: sync::Arc<atomic::AtomicBool>,
        connection: &ActiveConnection,
    ) -> Result<()> {
        let path = expand_path(
            apply_template(
                self.config
                    .path
                    .to_str()
                    .ok_or(anyhow!("Invalid path field in stream config"))?,
                timestamp,
            )
            .into(),
        );

        let file = io::BufReader::new(
            fs::File::open(&path)
                .with_context(|| format!("Failed to open file '{}'", path.display()))?,
        );

        let cmds: Vec<Command> = file
            .lines()
            .map_while(|line| Some(Command::from(line.ok()?)))
            .collect();

        let stream = || -> Result<Vec<Message>> {
            let mut receivers: Vec<channel::Receiver<Message>> = Vec::new();

            cmds.iter().try_for_each(|cmd| -> Result<()> {
                match cmd {
                    Command::Block(_) => {
                        if !running.load(atomic::Ordering::Relaxed) {
                            bail!("Stopped streaming early");
                        }
                        Ok(receivers.push(connection.send(cmd.clone())?))
                    }
                    Command::Realtime(_) => Ok(()),
                }
            })?;

            Ok(receivers
                .iter()
                .flat_map(|rx| rx.iter().collect::<Vec<Message>>())
                .collect())
        };

        if self.config.check {
            info!("Checking G-code for errors");

            // May need to implement further logic when enabling/disabling check mode to ensure
            // that Grbl is in the correct state. ie check parser state beforehand.
            connection.send(Command::Block("$C".to_string()))?.recv()?;

            // Potential issue here with the reported line number. Will be incorrect if Grbl
            // responds with anything more than a single 'ok' or 'error:{code}', as the responses
            // are flattened before the line index is recorded.
            let errors: Vec<(usize, Message)> = stream()?
                .into_iter()
                .enumerate()
                .filter_map(|(index, msg)| match msg {
                    Message::Response(Response::Error(_)) => Some((index + 1, msg)),
                    _ => None,
                })
                .collect();

            connection.send(Command::Block("$C".to_string()))?.recv()?;

            if errors.len() > 0 {
                bail!(
                    "Checking complete! {} errors found:\n{}\n",
                    errors.len(),
                    errors
                        .into_iter()
                        .fold(String::new(), |result, err| format!(
                            "{}\n{}:{} - {}",
                            result,
                            &path.display(),
                            err.0,
                            err.1
                        )),
                );
            } else {
                info!("Checking complete! No errors found");
            }
        }

        info!("Streaming G-code");
        let msgs = stream()?;

        if let Some(output_config) = &self.config.output {
            match output_config {
                OutputConfig {
                    kind: OutputKind::ProbedPoints,
                    ..
                } => {
                    let output_path = expand_path(
                        apply_template(
                            output_config
                                .path
                                .to_str()
                                .ok_or(anyhow!("Invalid path field in output config"))?,
                            timestamp,
                        )
                        .into(),
                    );
                    info!("Writing probed points to '{}'", output_path.display());

                    if let Some(parent) = path::Path::new(&output_path).parent() {
                        std::fs::create_dir_all(parent)?;
                    }

                    let mut output =
                        io::BufWriter::new(fs::File::create(&output_path).with_context(|| {
                            format!("Failed to create output file '{}'", output_path.display())
                        })?);

                    writeln!(output, "x,y,z")?;

                    msgs.into_iter().for_each(|msg| match msg {
                        Message::Push(Push::Feedback(Feedback { kind, data }, _))
                            if &kind == "PRB" =>
                        {
                            if let Err(err) = writeln!(output, "{}", data) {
                                error!(
                                    "Failed to write probed point to '{}': {}",
                                    output_path.display(),
                                    err
                                );
                            }
                        }
                        _ => (),
                    });
                }
            }
        }

        info!("Streaming complete! Waiting for execution to finish before proceeding...");
        connection
            .send(Command::Block("G4 P0.5".to_string()))?
            .recv()?;
        info!("G-code finished executing");

        Ok(())
    }
}

impl<'a> Task for Process<'a> {
    fn execute(
        &self,
        timestamp: &str,
        _: sync::Arc<atomic::AtomicBool>,
        _: &ActiveConnection,
    ) -> Result<()> {
        let cmd = apply_template(&self.config.command, timestamp);

        let output = process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .with_context(|| format!("Failed to execute command '{}'", cmd))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Command failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("Command output: {}", stdout.trim());
        }

        Ok(())
    }
}
