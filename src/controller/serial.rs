use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use log::error;

use super::command::Command;
use super::message::{Push, Report, Response};
use super::{Controller, ControllerError};

pub fn wait_for_report<F: Fn(&Report) -> bool>(
    controller: &Controller,
    predicate: Option<F>,
) -> Result<Option<Report>, ControllerError> {
    let Some((prio_stream_tx, prio_stream_rx)) = controller.prio_stream_channel.clone() else {
        return Err(ControllerError::SerialError(
            "Controller not started".to_string(),
        ));
    };

    let polling = Arc::new(AtomicBool::new(true));
    let running = controller.running.clone();

    Ok(thread::scope(|scope| {
        scope.spawn(|| {
            while polling.load(Ordering::Relaxed) {
                if let Err(error) = prio_stream_tx.send(Command::Realtime(b'?')) {
                    error!("Failed to poll status report: {}", error);
                }

                thread::sleep(Duration::from_millis(200));
            }
        });

        while running.load(Ordering::Relaxed) {
            match prio_stream_rx.recv() {
                Ok(Push::Report(report)) => {
                    if let Some(matcher) = &predicate {
                        if !matcher(&report) {
                            continue;
                        }
                    }

                    polling.store(false, Ordering::Relaxed);
                    return Ok(Some(report));
                }
                Err(error) => {
                    return Err(ControllerError::SerialError(format!(
                        "Failed to wait for status report: {}",
                        error
                    )));
                }
            }
        }

        Ok(None)
    })?)
}

pub fn buffered_stream(
    controller: &Controller,
    gcode: Vec<&str>,
    rx_buffer_size: usize,
) -> Result<Vec<(i32, Response)>, ControllerError> {
    let Some((stream_tx, stream_rx)) = controller.stream_channel.clone() else {
        return Err(ControllerError::SerialError(
            "Controller not started".to_string(),
        ));
    };

    let mut queued_bytes = VecDeque::new();
    let mut responses = Vec::new();

    let mut sent = 0;
    let mut received = 0;

    let mut receive =
        |received: &mut i32, queued_bytes: &mut VecDeque<usize>| -> Result<(), ControllerError> {
            let response = stream_rx.recv().map_err(|error| {
                ControllerError::SerialError(format!("Failed to wait for response: {}", error))
            })?;

            if let Response::Ok | Response::Error(_) = response {
                queued_bytes.pop_front();
                *received += 1;
            }

            responses.push((*received, response));

            Ok(())
        };

    for raw_line in gcode {
        let line = raw_line.trim();

        queued_bytes.push_back(line.len() + 1);
        sent += 1;

        while queued_bytes.iter().sum::<usize>() >= rx_buffer_size - 1 {
            receive(&mut received, &mut queued_bytes)?;
        }

        stream_tx
            .send(Command::Gcode(line.to_string()))
            .map_err(|error| {
                ControllerError::SerialError(format!("Failed to send G-code command: {}", error))
            })?;
    }

    while sent > received {
        receive(&mut received, &mut queued_bytes)?;
    }

    Ok(responses)
}
