//! MIDI transport: finds the beat bars ports and exchanges SysEx frames with them.
//!
//! Everything here blocks, so call it from a worker thread, never from the UI thread.

use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::protocol::{self, Command, Config, ConfigReader, DEVICE_NAMES, DeviceInfo};

/// How long to wait for the answer to one read.
const READ_TIMEOUT: Duration = Duration::from_millis(600);

/// How often a read is retried before the device counts as unresponsive.
const READ_ATTEMPTS: usize = 3;

const CLIENT_NAME: &str = "beatbars-config";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    NotFound,
    Midi(String),
    NoAnswer(Command),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound => write!(f, "No beat bars device found. Is it plugged in?"),
            Error::Midi(message) => write!(f, "MIDI error: {message}"),
            Error::NoAnswer(command) => write!(f, "The device did not answer ({command:?})."),
        }
    }
}

impl std::error::Error for Error {}

fn midi_error(error: impl std::fmt::Display) -> Error {
    Error::Midi(error.to_string())
}

/// Set `BEATBARS_DEBUG=1` to trace every frame on stderr.
fn debug() -> bool {
    static DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DEBUG.get_or_init(|| std::env::var_os("BEATBARS_DEBUG").is_some())
}

fn is_beatbars(name: &str) -> bool {
    let name = name.to_lowercase();
    DEVICE_NAMES.iter().any(|prefix| name.contains(prefix))
}

/// Names of every MIDI port, for diagnostics: `(inputs, outputs)`.
pub fn port_names() -> Result<(Vec<String>, Vec<String>)> {
    let input = MidiInput::new(CLIENT_NAME).map_err(midi_error)?;
    let output = MidiOutput::new(CLIENT_NAME).map_err(midi_error)?;
    let inputs = input.ports().iter().filter_map(|p| input.port_name(p).ok()).collect();
    let outputs = output.ports().iter().filter_map(|p| output.port_name(p).ok()).collect();
    Ok((inputs, outputs))
}

/// Listens to the device's MIDI output for `duration`, calling `on_message` for every message
/// that is not one of our SysEx answers. Used to check what the buttons and pedal really send.
pub fn monitor(duration: Duration, mut on_message: impl FnMut(&[u8]) + Send + 'static) -> Result<String> {
    let mut input = MidiInput::new(CLIENT_NAME).map_err(midi_error)?;
    input.ignore(Ignore::None);
    let port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| is_beatbars(&n)))
        .ok_or(Error::NotFound)?;
    let name = input.port_name(&port).map_err(midi_error)?;
    let connection = input
        .connect(&port, "beatbars-config-monitor", move |_, bytes, _| on_message(bytes), ())
        .map_err(midi_error)?;
    thread::sleep(duration);
    drop(connection);
    Ok(name)
}

/// A MIDI message in words, e.g. `ch 2  Control Change  #80 = 127`.
pub fn describe(bytes: &[u8]) -> String {
    let Some(&status) = bytes.first() else { return String::new() };
    let data = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let channel = (status & 0x0F) + 1;
    match status & 0xF0 {
        0x80 => format!("ch {channel:<2} Note Off        {} vel {}", protocol::note_name(data(1).into()), data(2)),
        0x90 => format!("ch {channel:<2} Note On         {} vel {}", protocol::note_name(data(1).into()), data(2)),
        0xA0 => format!("ch {channel:<2} Poly Pressure   {} = {}", protocol::note_name(data(1).into()), data(2)),
        0xB0 => format!("ch {channel:<2} Control Change  #{} = {}", data(1), data(2)),
        0xC0 => format!("ch {channel:<2} Program Change  {}", u16::from(data(1)) + 1),
        0xD0 => format!("ch {channel:<2} Channel Pressure {}", data(1)),
        0xE0 => {
            let value = (i32::from(data(2)) << 7 | i32::from(data(1))) - 8192;
            format!("ch {channel:<2} Pitch Bend      {value}")
        }
        _ => match status {
            0xF0 => format!("SysEx {bytes:02X?}"),
            0xF8 => "Timing Clock".to_owned(),
            0xFA => "Start".to_owned(),
            0xFB => "Continue".to_owned(),
            0xFC => "Stop".to_owned(),
            other => format!("{other:02X} {:02X?}", &bytes[1..]),
        },
    }
}

/// Whether a beat bars output port is currently present.
pub fn device_present() -> bool {
    let Ok(output) = MidiOutput::new(CLIENT_NAME) else { return false };
    output
        .ports()
        .iter()
        .any(|p| output.port_name(p).is_ok_and(|n| is_beatbars(&n)))
}

/// An open connection to a beat bars device.
pub struct Device {
    pub name: String,
    output: Mutex<MidiOutputConnection>,
    incoming: Mutex<Receiver<Vec<u8>>>,
    _input: Mutex<MidiInputConnection<()>>,
}

impl Device {
    /// Opens the first beat bars device found.
    pub fn open() -> Result<Device> {
        let mut input = MidiInput::new(CLIENT_NAME).map_err(midi_error)?;
        input.ignore(Ignore::None);
        let output = MidiOutput::new(CLIENT_NAME).map_err(midi_error)?;

        let in_port = input
            .ports()
            .into_iter()
            .find(|p| input.port_name(p).is_ok_and(|n| is_beatbars(&n)))
            .ok_or(Error::NotFound)?;
        let out_port = output
            .ports()
            .into_iter()
            .find(|p| output.port_name(p).is_ok_and(|n| is_beatbars(&n)))
            .ok_or(Error::NotFound)?;
        let name = output.port_name(&out_port).map_err(midi_error)?;

        let (sender, incoming) = mpsc::channel();
        // CoreMIDI may hand a SysEx over in pieces, so frames are reassembled here.
        let mut pending: Vec<u8> = Vec::new();
        let input = input
            .connect(
                &in_port,
                "beatbars-config-in",
                move |_, bytes, _| {
                    if debug() {
                        eprintln!("midi in: {bytes:02X?}");
                    }
                    for &byte in bytes {
                        match byte {
                            0xF0 => {
                                pending.clear();
                                pending.push(byte);
                            }
                            0xF7 if !pending.is_empty() => {
                                pending.push(byte);
                                let _ = sender.send(std::mem::take(&mut pending));
                            }
                            // Real-time bytes may be interleaved anywhere; ignore them.
                            0xF8..=0xFF => {}
                            // Any other status byte aborts an unfinished SysEx.
                            0x80..=0xEF => pending.clear(),
                            _ if !pending.is_empty() => pending.push(byte),
                            _ => {}
                        }
                    }
                },
                (),
            )
            .map_err(midi_error)?;
        let output = output.connect(&out_port, "beatbars-config-out").map_err(midi_error)?;

        Ok(Device {
            name,
            output: Mutex::new(output),
            incoming: Mutex::new(incoming),
            _input: Mutex::new(input),
        })
    }

    fn send(&self, frame: &[u8]) -> Result<()> {
        if debug() {
            eprintln!("midi out: {frame:02X?}");
        }
        self.output
            .lock()
            .expect("midi output lock")
            .send(frame)
            .map_err(midi_error)
    }

    /// Throws away anything that arrived unasked.
    fn drain(&self) {
        let incoming = self.incoming.lock().expect("midi input lock");
        while incoming.try_recv().is_ok() {}
    }

    /// Waits for a response to `command`, skipping anything else.
    fn wait_for(&self, command: Command, timeout: Duration) -> Option<Vec<u8>> {
        let incoming = self.incoming.lock().expect("midi input lock");
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            match incoming.recv_timeout(left) {
                Ok(frame) => {
                    if let Some((code, values)) = protocol::split_response(&frame)
                        && code == command as u8
                    {
                        return Some(values.to_vec());
                    }
                }
                Err(error) => {
                    if debug() {
                        eprintln!("midi wait for {command:?}: {error:?}");
                    }
                    return None;
                }
            }
        }
    }

    /// Sends raw bytes and collects every SysEx frame that comes back within `window`.
    pub fn exchange_raw(&self, frame: &[u8], window: Duration) -> Result<Vec<Vec<u8>>> {
        self.drain();
        self.send(frame)?;
        let incoming = self.incoming.lock().expect("midi input lock");
        let deadline = Instant::now() + window;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match incoming.recv_timeout(left) {
                Ok(frame) => out.push(frame),
                Err(_) => break,
            }
        }
        Ok(out)
    }

    /// Reads `count` bytes starting at register `address`, whatever is mapped there.
    pub fn query_raw(&self, address: u8, count: u8) -> Result<Vec<u8>> {
        let frame = [0xF0, 0x00, 0x21, 0x3A, address & 0x7F, count, 1, 0xF7];
        let answers = self.exchange_raw(&frame, READ_TIMEOUT)?;
        answers
            .iter()
            .filter_map(|f| protocol::split_response(f))
            .find(|(code, _)| *code == address)
            .map(|(_, values)| values.to_vec())
            .ok_or(Error::Midi(format!("no answer at register {address}")))
    }

    /// Sends one read and returns the value bytes of its answer.
    pub fn query(&self, command: Command, expect: u8) -> Result<Vec<u8>> {
        for _ in 0..READ_ATTEMPTS {
            self.drain();
            self.send(&protocol::read(command, expect))?;
            if let Some(values) = self.wait_for(command, READ_TIMEOUT) {
                return Ok(values);
            }
        }
        Err(Error::NoAnswer(command))
    }

    /// Reads the device identity and its whole configuration.
    pub fn read_config(&self) -> Result<(DeviceInfo, Config)> {
        let mut reader = ConfigReader::default();
        for (command, expect) in protocol::read_requests() {
            let values = self.query(command, expect)?;
            reader.apply(command as u8, &values);
        }
        let info = reader.info.ok_or(Error::NoAnswer(Command::Info))?;
        Ok((info, reader.finish()))
    }

    /// Writes a whole configuration, pacing the frames like the original app.
    pub fn write_config(&self, config: &Config) -> Result<()> {
        for frame in config.write_messages() {
            self.send(&frame)?;
            thread::sleep(protocol::WRITE_DELAY);
        }
        Ok(())
    }

    /// Restores factory settings.
    pub fn factory_reset(&self) -> Result<()> {
        self.send(&protocol::execute(Command::FactoryReset))?;
        thread::sleep(protocol::FACTORY_RESET_DELAY);
        Ok(())
    }

    /// Runs the pedal calibration. Returns whether the device confirmed it finished.
    pub fn calibrate(&self) -> Result<bool> {
        self.drain();
        self.send(&protocol::execute(Command::PedalCalibrate))?;
        let confirmed = self
            .wait_for(Command::PedalCalibrate, protocol::CALIBRATION_TIME + Duration::from_secs(4))
            .is_some();
        Ok(confirmed)
    }
}
