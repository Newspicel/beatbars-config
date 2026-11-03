//! beat bars config: a replacement for the original configuration app.
//!
//! Without arguments it opens the window. A few diagnostic subcommands talk to the hardware
//! directly from the terminal; `BEATBARS_DEBUG=1` traces every SysEx frame on stderr.

mod app;
mod midi;
mod protocol;
mod ui;

use std::process::ExitCode;
use std::time::Duration;

use zgui::prelude::*;
use zgui::view;

const USAGE: &str = "\
usage: beatbars-config [COMMAND]

  (no command)   open the configuration window
  ports          list MIDI ports
  dump           print the device's configuration
  registers      print the device's raw register blocks
  monitor [SECS] print what the device sends (default 30 s)
  raw HEX...     send raw SysEx frames (e.g. \"F0 00 21 3A 01 03 01 F7\") and print the answers";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return match gui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        };
    };
    match cli(command.trim_start_matches('-'), &args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn gui() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.newspicel.beatbars-config")
        .with_title("beat bars config")
        .with_size(680.0, 860.0)
        .with_stylesheet(ui::SHEET)
        .run(|| view! { App() })
}

fn cli(command: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        "ports" => {
            let (inputs, outputs) = midi::port_names()?;
            println!("inputs:  {inputs:?}\noutputs: {outputs:?}");
        }
        "dump" => {
            let device = midi::Device::open()?;
            let (info, config) = device.read_config()?;
            println!("device: {} ({})\n{config:#?}", device.name, info.summary());
        }
        "registers" => {
            let device = midi::Device::open()?;
            print!("{}", registers(&device));
        }
        "monitor" => {
            let seconds = rest.first().map(|s| s.parse()).transpose()?.unwrap_or(30);
            println!("Listening for {seconds} s. Press the buttons / move the pedal.");
            let start = std::time::Instant::now();
            let name = midi::monitor(Duration::from_secs(seconds), move |bytes| {
                println!("{:6.2}s  {}", start.elapsed().as_secs_f32(), midi::describe(bytes));
            })?;
            println!("(listened to {name})");
        }
        "raw" => {
            let device = midi::Device::open()?;
            for hex in rest {
                let bytes = hex
                    .split([' ', ',', ':'])
                    .filter(|s| !s.is_empty())
                    .map(|b| u8::from_str_radix(b, 16))
                    .collect::<Result<Vec<u8>, _>>()?;
                let answers = device.exchange_raw(&bytes, Duration::from_millis(500))?;
                println!("{bytes:02X?} -> {answers:02X?}");
            }
        }
        "h" | "help" => println!("{USAGE}"),
        other => return Err(format!("unknown command `{other}`\n\n{USAGE}").into()),
    }
    Ok(())
}

/// Raw dump of every register block the device answers on.
fn registers(device: &midi::Device) -> String {
    let mut out = String::new();
    for start in [0u8, 3, 8, 21, 23, 28, 41, 48, 61, 70] {
        let values = device.query_raw(start, 16).unwrap_or_default();
        out += &format!("{start:3}: {values:02X?}\n");
    }
    out
}

use ui::AppProps;
