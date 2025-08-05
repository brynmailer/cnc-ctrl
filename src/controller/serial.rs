use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use log::error;
use regex::Regex;

use super::command::Command;
use super::message::{Push, Report, Response, Status};
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

    Ok(thread::scope(|scope| {
        scope.spawn(|| {
            while polling.load(Ordering::Relaxed) && controller.running.load(Ordering::Relaxed) {
                if let Err(error) = prio_serial_tx.send(Command::Realtime(b'?')) {
                    error!("Failed to poll status report: {}", error);
                }

                thread::sleep(Duration::from_millis(200));
            }
        });

        while controller.running.load(Ordering::Relaxed) {
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
    mut file: Option<&mut File>,
) -> Result<Vec<(i32, Response)>, ControllerError> {
    let Some((serial_tx, serial_rx)) = controller.serial_channel.clone() else {
        return Err(ControllerError::SerialError(
            "Controller not started".to_string(),
        ));
    };

    let re = Regex::new(r"^\$J=.* IN$").expect("Failed to create regex");
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
        let interruptible = re.is_match(raw_line.trim());
        let line = if interruptible {
            raw_line.trim().strip_suffix(" IN").unwrap()
        } else {
            raw_line.trim()
        };

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

        if interruptible {
            let report = wait_for_report(
                controller,
                Some(|report: &Report| {
                    matches!(
                        report,
                        &Report {
                            status: Some(Status::Idle),
                            mpos: Some(_),
                            ..
                        }
                    )
                }),
            )?;

            if let Some(report) = report {
                let unwrapped_mpos = report.mpos.unwrap();
                if let Some(file) = &mut file {
                    file.write_all(
                        format!(
                            "{},{},{}\n",
                            unwrapped_mpos.0, unwrapped_mpos.1, unwrapped_mpos.2
                        )
                        .as_bytes(),
                    )
                    .map_err(|error| {
                        ControllerError::SerialError(format!("Failed to save point: {}", error))
                    })?;
                }
            }
        }
    }

    while sent_count > received_count {
        receive(&mut received_count, &mut bytes_queued)?;
    }

    Ok(responses)
}
