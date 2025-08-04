use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use regex::Regex;

use super::Controller;
use super::command::Command;
use super::message::{Push, Report, Response, Status};

pub fn wait_for_report<F: Fn(&Report) -> bool>(
    controller: &Controller,
    predicate: Option<F>,
) -> Option<Report> {
    let Some((prio_serial_tx, prio_serial_rx)) = controller.prio_serial_channel.clone() else {
        panic!("Failed to clone serial: Controller not started");
    };

    let polling = Arc::new(AtomicBool::new(true));
    let running = controller.running.clone();

    thread::scope(|scope| {
        scope.spawn(|| {
            while polling.load(Ordering::Relaxed) {
                prio_serial_tx
                    .send(Command::Realtime(b'?'))
                    .expect("Failed to poll grbl status report");

                thread::sleep(Duration::from_millis(200));
            }
        });

        loop {
            if !running.load(Ordering::Relaxed) {
                return None;
            }

            match prio_serial_rx.recv() {
                Ok(Push::Report(report)) => {
                    if let Some(matcher) = &predicate {
                        if !matcher(&report) {
                            continue;
                        }
                    }

                    polling.store(false, Ordering::Relaxed);
                    return Some(report);
                }
                Err(err) => panic!("Failed to wait for interrupt: {}", err),
            }
        }
    })
}

pub fn buffered_stream(
    controller: &Controller,
    gcode: Vec<&str>,
    rx_buffer_size: usize,
    mut file: Option<&mut File>,
) -> Result<Vec<(i32, Response)>, Box<dyn std::error::Error>> {
    let Some((serial_tx, serial_rx)) = controller.serial_channel.clone() else {
        panic!("Failed to stream G-code: Controller not started");
    };

    let re = Regex::new(r"^\$J=.* IN$")?;
    let mut bytes_queued = VecDeque::new();
    let mut received_count = 0;
    let mut sent_count = 0;

    let mut responses = Vec::new();

    let mut receive = |received_count: &mut i32,
                       bytes_queued: &mut VecDeque<usize>|
     -> Result<(), Box<dyn std::error::Error>> {
        let response = serial_rx.recv()?;
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

        serial_tx.send(Command::Gcode(line.to_string()))?;
        sent_count += 1;

        if interruptible {
            if let Some(report) = wait_for_report(
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
            ) {
                let unwrapped_mpos = report.mpos.unwrap();
                if let Some(file) = &mut file {
                    file.write_all(
                        format!(
                            "{},{},{}\n",
                            unwrapped_mpos.0, unwrapped_mpos.1, unwrapped_mpos.2
                        )
                        .as_bytes(),
                    )
                    .expect("Failed to write point to file");
                }
            }
        }
    }

    while sent_count > received_count {
        receive(&mut received_count, &mut bytes_queued)?;
    }

    Ok(responses)
}
