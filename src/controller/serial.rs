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
    let Some((prio_serial_tx, prio_serial_rx)) = controller.prio_serial_channel.clone() else {
        return Err(ControllerError::SerialError(
            "Controller not started".to_string(),
        ));
    };

    let polling = Arc::new(AtomicBool::new(true));
    let running = controller.running.clone();

    Ok(thread::scope(|scope| {
        scope.spawn(|| {
            while polling.load(Ordering::Relaxed) {
                if let Err(error) = prio_serial_tx.send(Command::Realtime(b'?')) {
                    error!("Failed to poll status report: {}", error);
                }

                thread::sleep(Duration::from_millis(200));
            }
        });

        while running.load(Ordering::Relaxed) {
            match prio_serial_rx.recv() {
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
    let Some((serial_tx, serial_rx)) = controller.serial_channel.clone() else {
        return Err(ControllerError::SerialError(
            "Controller not started".to_string(),
        ));
    };

    let mut bytes_queued = VecDeque::new();
    let mut received_count = 0;
    let mut sent_count = 0;

    let mut responses = Vec::new();

    let mut receive = |received_count: &mut i32,
                       bytes_queued: &mut VecDeque<usize>|
     -> Result<(), ControllerError> {
        let response = serial_rx.recv().map_err(|error| {
            ControllerError::SerialError(format!("Failed to wait for response: {}", error))
        })?;

        *received_count += 1;
        bytes_queued.pop_front();

        responses.push((*received_count, response));

        Ok(())
    };

    for raw_line in gcode {
        let line = raw_line.trim();

        bytes_queued.push_back(line.len());

        while bytes_queued.iter().sum::<usize>() >= rx_buffer_size {
            receive(&mut received_count, &mut bytes_queued)?;
        }

        serial_tx
            .send(Command::Gcode(line.to_string()))
            .map_err(|error| {
                ControllerError::SerialError(format!("Failed to send G-code command: {}", error))
            })?;
        sent_count += 1;
    }

    while sent_count > received_count {
        receive(&mut received_count, &mut bytes_queued)?;
    }

    Ok(responses)
}
