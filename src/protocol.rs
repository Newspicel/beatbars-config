//! The beat bars SysEx configuration protocol.
//!
//! Reverse engineered from `beatbars config` 1.4.2 (the JavaFX app shipped by NILSON sp. z o.o.,
//! `com.beatbars.adapters`). Everything here is pure data and byte shuffling. No MIDI I/O happens
//! in this module.
//!
//! # Wire format
//!
//! Request (host → device):
//!
//! ```text
//! F0 00 21 3A <command> <count> <access> <params…> F7
//! ```
//!
//! * `command` is one of [`Command`].
//! * `access` is `1` read, `2` write, `3` execute.
//! * `count` is the number of parameter bytes on a write and the number of bytes expected back on
//!   a read.
//!
//! Response (device → host):
//!
//! ```text
//! F0 00 21 3A <command> <values…> F7
//! ```
//!
//! MIDI status bytes are not 7-bit clean, so they are sent shifted down by 127
//! (see [`encode_status`] / [`decode_status`]).

use std::fmt;
use std::time::Duration;

/// Manufacturer ID that prefixes every message.
pub const SYSEX_ID: [u8; 3] = [0x00, 0x21, 0x3A];

/// MIDI port name prefixes the original app looks for.
pub const DEVICE_NAMES: [&str; 2] = ["beat bars adapter", "beat bars control"];

/// Pause between two writes. The original app always waits this long.
pub const WRITE_DELAY: Duration = Duration::from_millis(160);

/// Pause after a factory reset before the device is asked anything again.
pub const FACTORY_RESET_DELAY: Duration = Duration::from_millis(800);

/// How long the device needs to calibrate a pedal.
pub const CALIBRATION_TIME: Duration = Duration::from_secs(9);

// ------------------------------------------------------------------------------------------------
// Commands and framing
// ------------------------------------------------------------------------------------------------

/// A configuration register / command on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Command {
    Info = 1,
    PedalMode = 8,
    PedalRange = 9,
    PedalMessageWrite = 11,
    PedalMessageRead = 12,
    PedalReversed = 16,
    PedalCalibrate = 20,
    Switch1Mode = 28,
    Switch1Range = 29,
    Switch1Message1Write = 31,
    Switch1Message1Read = 32,
    Switch1Step = 36,
    Switch1ButtonType = 38,
    Switch1ButtonMode = 39,
    Switch1Message2Write = 41,
    Switch1Message2Read = 42,
    Switch2Mode = 48,
    Switch2Range = 49,
    Switch2Message1Write = 51,
    Switch2Message1Read = 52,
    Switch2Step = 56,
    Switch2ButtonType = 58,
    Switch2ButtonMode = 59,
    Switch2Message2Write = 61,
    Switch2Message2Read = 62,
    SwitchesLinked = 70,
    FactoryReset = 127,
}

impl Command {
    pub const ALL: [Command; 27] = [
        Command::Info,
        Command::PedalMode,
        Command::PedalRange,
        Command::PedalMessageWrite,
        Command::PedalMessageRead,
        Command::PedalReversed,
        Command::PedalCalibrate,
        Command::Switch1Mode,
        Command::Switch1Range,
        Command::Switch1Message1Write,
        Command::Switch1Message1Read,
        Command::Switch1Step,
        Command::Switch1ButtonType,
        Command::Switch1ButtonMode,
        Command::Switch1Message2Write,
        Command::Switch1Message2Read,
        Command::Switch2Mode,
        Command::Switch2Range,
        Command::Switch2Message1Write,
        Command::Switch2Message1Read,
        Command::Switch2Step,
        Command::Switch2ButtonType,
        Command::Switch2ButtonMode,
        Command::Switch2Message2Write,
        Command::Switch2Message2Read,
        Command::SwitchesLinked,
        Command::FactoryReset,
    ];

    pub fn from_code(code: u8) -> Option<Command> {
        Command::ALL.into_iter().find(|c| *c as u8 == code)
    }
}

/// What a request asks the device to do with a [`Command`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Access {
    Read = 1,
    Write = 2,
    Execute = 3,
}

/// Builds one request frame.
pub fn frame(command: Command, access: Access, count: u8, params: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + params.len());
    out.push(0xF0);
    out.extend_from_slice(&SYSEX_ID);
    out.push(command as u8);
    out.push(count);
    out.push(access as u8);
    out.extend(params.iter().map(|b| b & 0x7F));
    out.push(0xF7);
    out
}

/// A write request carrying `params`.
pub fn write(command: Command, params: &[u8]) -> Vec<u8> {
    frame(command, Access::Write, params.len() as u8, params)
}

/// A read request that expects `expect` value bytes back.
pub fn read(command: Command, expect: u8) -> Vec<u8> {
    frame(command, Access::Read, expect, &[])
}

/// An execute request (calibrate, factory reset).
pub fn execute(command: Command) -> Vec<u8> {
    frame(command, Access::Execute, 0, &[])
}

/// Splits a device → host frame into its command code and value bytes.
///
/// Returns `None` for anything that is not one of our SysEx frames.
pub fn split_response(raw: &[u8]) -> Option<(u8, &[u8])> {
    if raw.len() < 5 || raw[0] != 0xF0 || raw[1..4] != SYSEX_ID {
        return None;
    }
    let body = &raw[4..];
    let body = body.strip_suffix(&[0xF7]).unwrap_or(body);
    let (&command, values) = body.split_first()?;
    Some((command, values))
}

/// Encodes a MIDI status byte (with its channel, 1-based) the way the device stores it.
pub fn encode_status(status: u8, channel: u8) -> u8 {
    let byte = if status < 0xF0 {
        (status & 0xF0) | (channel.saturating_sub(1) & 0x0F)
    } else {
        status
    };
    byte.wrapping_sub(127) & 0x7F
}

/// Decodes a status byte read from the device into `(status without channel, channel 1..=16)`.
pub fn decode_status(byte: u8) -> (u8, u8) {
    let status = byte.wrapping_add(127);
    if status < 0xF0 {
        (status & 0xF0, (status & 0x0F) + 1)
    } else {
        (status, 1)
    }
}

// ------------------------------------------------------------------------------------------------
// Device identity
// ------------------------------------------------------------------------------------------------

/// What kind of beat bars hardware answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    PedalAdapter,
    SwitchAdapter,
    DualAdapter,
    Pedal,
    Switch,
    Dual,
    Unknown(u8),
}

impl DeviceType {
    /// `0` means the device did not say.
    pub fn from_code(code: u8) -> Option<DeviceType> {
        Some(match code {
            0 => return None,
            1 => DeviceType::PedalAdapter,
            2 => DeviceType::SwitchAdapter,
            3 => DeviceType::DualAdapter,
            5 => DeviceType::Pedal,
            6 => DeviceType::Switch,
            7 => DeviceType::Dual,
            other => DeviceType::Unknown(other),
        })
    }

    /// Whether the device has an expression pedal input.
    pub fn has_pedal(self) -> bool {
        matches!(
            self,
            DeviceType::PedalAdapter
                | DeviceType::DualAdapter
                | DeviceType::Pedal
                | DeviceType::Dual
                | DeviceType::Unknown(_)
        )
    }

    /// Whether the device has the two foot switch inputs.
    pub fn has_switches(self) -> bool {
        matches!(
            self,
            DeviceType::SwitchAdapter
                | DeviceType::DualAdapter
                | DeviceType::Switch
                | DeviceType::Dual
                | DeviceType::Unknown(_)
        )
    }

    /// Adapters take third-party switches, so the switch type has to be configured.
    pub fn is_adapter(self) -> bool {
        matches!(
            self,
            DeviceType::PedalAdapter
                | DeviceType::SwitchAdapter
                | DeviceType::DualAdapter
                | DeviceType::Unknown(_)
        )
    }

    pub fn label(self) -> String {
        match self {
            DeviceType::PedalAdapter => "Pedal adapter".into(),
            DeviceType::SwitchAdapter => "Switch adapter".into(),
            DeviceType::DualAdapter => "Pedal + switch adapter".into(),
            DeviceType::Pedal => "Pedal".into(),
            DeviceType::Switch => "Switch".into(),
            DeviceType::Dual => "Pedal + switch".into(),
            DeviceType::Unknown(code) => format!("unknown type {code}"),
        }
    }
}

/// Firmware version as reported by [`Command::Info`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Firmware {
    pub major: u8,
    pub minor: u8,
}

impl Firmware {
    pub const fn new(major: u8, minor: u8) -> Self {
        Firmware { major, minor }
    }

    /// Note On toggles on the switches arrived in 1.1.
    pub fn has_note_toggle(self) -> bool {
        self >= Firmware::new(1, 1)
    }

    /// MIDI clock on the pedal and real-time messages on the switches arrived in 1.2.
    pub fn has_realtime(self) -> bool {
        self >= Firmware::new(1, 2)
    }

    /// Latching / non-latching switch support arrived in 1.3.
    pub fn has_button_type(self) -> bool {
        self >= Firmware::new(1, 3)
    }
}

impl fmt::Display for Firmware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// What [`Command::Info`] returned.
///
/// Original firmware answers `type, major, minor`. Some units (seen on a "beat bars controller")
/// only store a single zero there, so both halves are optional. Missing information enables
/// everything rather than hiding controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    pub device_type: Option<DeviceType>,
    pub firmware: Option<Firmware>,
}

impl DeviceInfo {
    pub fn from_values(values: &[u8]) -> DeviceInfo {
        DeviceInfo {
            device_type: values.first().copied().and_then(DeviceType::from_code),
            firmware: match values {
                [_, major, minor, ..] => Some(Firmware::new(*major, *minor)),
                _ => None,
            },
        }
    }

    pub fn has_switches(&self) -> bool {
        self.device_type.is_none_or(DeviceType::has_switches)
    }

    pub fn has_button_type(&self) -> bool {
        self.device_type.is_none_or(DeviceType::is_adapter)
            && self.firmware.is_none_or(Firmware::has_button_type)
    }

    /// Firmware to gate features on; unknown firmware gets everything.
    pub fn firmware_or_latest(&self) -> Firmware {
        self.firmware.unwrap_or(Firmware::new(u8::MAX, u8::MAX))
    }

    pub fn summary(&self) -> String {
        let kind = self.device_type.map_or_else(|| "type not reported".to_owned(), DeviceType::label);
        match self.firmware {
            Some(firmware) => format!("{kind} · firmware {firmware}"),
            None => kind,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// MIDI messages a control can send
// ------------------------------------------------------------------------------------------------

/// Which names to show beside a parameter number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamNames {
    None,
    Controller,
    Note,
}

impl ParamNames {
    pub fn name(self, value: i32) -> &'static str {
        match self {
            ParamNames::None => "",
            ParamNames::Controller => cc_name(value),
            ParamNames::Note => note_name(value),
        }
    }
}

/// The MIDI message type a pedal or switch sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageType {
    NoteOn,
    PolyPressure,
    ControlChange,
    ProgramChange,
    ChannelPressure,
    PitchBend,
    /// MIDI clock (tempo) driven by the pedal.
    Clock,
    /// System real-time (tap tempo, start/stop, continue/stop) from a switch.
    RealTime,
}

impl MessageType {
    /// What a pedal can send, in the order the original app lists them.
    pub const PEDAL: [MessageType; 5] = [
        MessageType::PolyPressure,
        MessageType::ControlChange,
        MessageType::ChannelPressure,
        MessageType::PitchBend,
        MessageType::Clock,
    ];

    /// What a switch can send, in the order the original app lists them.
    pub const SWITCH: [MessageType; 4] = [
        MessageType::ControlChange,
        MessageType::ProgramChange,
        MessageType::NoteOn,
        MessageType::RealTime,
    ];

    pub fn pedal_choices(firmware: Firmware) -> Vec<MessageType> {
        MessageType::PEDAL
            .into_iter()
            .filter(|m| *m != MessageType::Clock || firmware.has_realtime())
            .collect()
    }

    pub fn switch_choices(firmware: Firmware) -> Vec<MessageType> {
        MessageType::SWITCH
            .into_iter()
            .filter(|m| *m != MessageType::NoteOn || firmware.has_note_toggle())
            .filter(|m| *m != MessageType::RealTime || firmware.has_realtime())
            .collect()
    }

    /// The MIDI status byte (channel 1).
    pub fn status(self) -> u8 {
        match self {
            MessageType::NoteOn => 0x90,
            MessageType::PolyPressure => 0xA0,
            MessageType::ControlChange => 0xB0,
            MessageType::ProgramChange => 0xC0,
            MessageType::ChannelPressure => 0xD0,
            MessageType::PitchBend => 0xE0,
            MessageType::Clock => 0xF8,
            MessageType::RealTime => 0xF9,
        }
    }

    pub fn from_status(status: u8) -> Option<MessageType> {
        match status {
            0x90 => Some(MessageType::NoteOn),
            0xA0 => Some(MessageType::PolyPressure),
            0xB0 => Some(MessageType::ControlChange),
            0xC0 => Some(MessageType::ProgramChange),
            0xD0 => Some(MessageType::ChannelPressure),
            0xE0 => Some(MessageType::PitchBend),
            0xF8 => Some(MessageType::Clock),
            0xF9 => Some(MessageType::RealTime),
            _ => None,
        }
    }

    /// USB-MIDI code index number of the packet the device emits.
    fn usb_cin(self) -> u8 {
        match self {
            MessageType::NoteOn => 9,
            MessageType::PolyPressure => 10,
            MessageType::ControlChange => 11,
            MessageType::ProgramChange => 12,
            MessageType::ChannelPressure => 13,
            MessageType::PitchBend => 14,
            MessageType::Clock | MessageType::RealTime => 5,
        }
    }

    /// 0 = single, 1 = fine (14 bit), 2 = double (tempo).
    fn range_factor_code(self) -> u8 {
        match self {
            MessageType::PitchBend => 1,
            MessageType::Clock => 2,
            _ => 0,
        }
    }

    /// Pedal min/max are stored divided by this.
    pub fn range_divisor(self) -> i32 {
        if self == MessageType::Clock { 2 } else { 1 }
    }

    /// 0 = three bytes, 1 = two bytes, 2 = one byte.
    fn length_code(self) -> u8 {
        match self {
            MessageType::ProgramChange | MessageType::ChannelPressure => 1,
            MessageType::Clock | MessageType::RealTime => 2,
            _ => 0,
        }
    }

    /// Whether this message carries a MIDI channel.
    pub fn has_channel(self) -> bool {
        !matches!(self, MessageType::Clock | MessageType::RealTime)
    }

    pub fn is_fine(self) -> bool {
        self == MessageType::PitchBend
    }

    pub fn pedal_mode(self) -> PedalMode {
        match self {
            MessageType::PolyPressure => PedalMode::PolyPressure,
            MessageType::ChannelPressure => PedalMode::ChannelPressure,
            MessageType::PitchBend => PedalMode::PitchBend,
            MessageType::Clock => PedalMode::Tempo,
            _ => PedalMode::ControlChange,
        }
    }

    pub fn switch_modes(self) -> &'static [SwitchMode] {
        match self {
            MessageType::ControlChange => &[
                SwitchMode::CcOnOff,
                SwitchMode::CcImpulse,
                SwitchMode::CcToggle,
                SwitchMode::CcUpDown,
            ],
            MessageType::ProgramChange => &[
                SwitchMode::PcImpulse,
                SwitchMode::PcToggle,
                SwitchMode::PcUpDown,
            ],
            MessageType::NoteOn => &[SwitchMode::NoteToggle],
            MessageType::RealTime => &[
                SwitchMode::TapTempo,
                SwitchMode::StartStop,
                SwitchMode::ContinueStop,
            ],
            _ => &[],
        }
    }

    /// The five bytes of a `*_MESSAGE_W` write.
    fn message_bytes(self, channel: u8, param: u8) -> [u8; 5] {
        [
            self.usb_cin(),
            encode_status(self.status(), channel),
            param & 0x7F,
            self.range_factor_code(),
            self.length_code(),
        ]
    }

    /// Stable identifier, used as a select value.
    pub fn key(self) -> &'static str {
        match self {
            MessageType::NoteOn => "note-on",
            MessageType::PolyPressure => "poly-pressure",
            MessageType::ControlChange => "cc",
            MessageType::ProgramChange => "pc",
            MessageType::ChannelPressure => "channel-pressure",
            MessageType::PitchBend => "pitch-bend",
            MessageType::Clock => "clock",
            MessageType::RealTime => "realtime",
        }
    }

    pub fn from_key(key: &str) -> Option<MessageType> {
        [
            MessageType::NoteOn,
            MessageType::PolyPressure,
            MessageType::ControlChange,
            MessageType::ProgramChange,
            MessageType::ChannelPressure,
            MessageType::PitchBend,
            MessageType::Clock,
            MessageType::RealTime,
        ]
        .into_iter()
        .find(|m| m.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            MessageType::NoteOn => "Note On",
            MessageType::PolyPressure => "Polyphonic Key Pressure (Aftertouch)",
            MessageType::ControlChange => "Control Change",
            MessageType::ProgramChange => "Program Change",
            MessageType::ChannelPressure => "Channel Pressure (Aftertouch)",
            MessageType::PitchBend => "Pitch Bend Change",
            MessageType::Clock => "Timing Clock (Tempo)",
            MessageType::RealTime => "System Real-Time",
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Pedal
// ------------------------------------------------------------------------------------------------

/// How the pedal's travel is mapped. One per pedal message type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PedalMode {
    PolyPressure = 1,
    ControlChange = 2,
    ChannelPressure = 3,
    PitchBend = 4,
    Tempo = 5,
}

/// A numeric parameter (controller number, note, program …) and how to show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamSpec {
    pub label: &'static str,
    pub min: i32,
    pub max: i32,
    pub default: i32,
    pub names: ParamNames,
}

impl PedalMode {
    pub fn param(self) -> Option<ParamSpec> {
        match self {
            PedalMode::PolyPressure => Some(ParamSpec {
                label: "Note #",
                min: 0,
                max: 127,
                default: 60,
                names: ParamNames::Note,
            }),
            PedalMode::ControlChange => Some(ParamSpec {
                label: "Controller #",
                min: 0,
                max: 119,
                default: 1,
                names: ParamNames::Controller,
            }),
            _ => None,
        }
    }

    pub fn min_label(self) -> &'static str {
        match self {
            PedalMode::PitchBend => "Min (coarse)",
            PedalMode::Tempo => "Min tempo",
            _ => "Min",
        }
    }

    pub fn max_label(self) -> &'static str {
        match self {
            PedalMode::PitchBend => "Max (coarse)",
            PedalMode::Tempo => "Max tempo",
            _ => "Max",
        }
    }

    /// Lowest value min may take.
    pub fn range_min(self) -> i32 {
        if self == PedalMode::Tempo { 10 } else { 0 }
    }

    /// Highest value max may take.
    pub fn range_max(self) -> i32 {
        if self == PedalMode::Tempo { 254 } else { 127 }
    }
}

/// 14-bit pitch bend value the coarse `value` stands for (as the original app shows it).
pub fn fine_pitch(value: i32, is_max: bool) -> i32 {
    if is_max {
        (value + 1) * 128 - 1 - 8192
    } else {
        value * 128 - 8192
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PedalConfig {
    pub message: MessageType,
    /// 1..=16
    pub channel: u8,
    pub param: i32,
    pub min: i32,
    pub max: i32,
    pub reversed: bool,
}

impl Default for PedalConfig {
    fn default() -> Self {
        let mut pedal = PedalConfig {
            message: MessageType::ControlChange,
            channel: 1,
            param: 0,
            min: 0,
            max: 0,
            reversed: false,
        };
        pedal.set_message(MessageType::ControlChange);
        pedal
    }
}

impl PedalConfig {
    pub fn mode(&self) -> PedalMode {
        self.message.pedal_mode()
    }

    /// Switches message type and resets the dependent values to the mode's defaults.
    pub fn set_message(&mut self, message: MessageType) {
        self.message = message;
        let mode = self.mode();
        self.param = mode.param().map_or(0, |p| p.default);
        self.min = mode.range_min();
        self.max = mode.range_max();
    }

    /// Sets min, pushing max up if they would cross.
    pub fn set_min(&mut self, value: i32) {
        let mode = self.mode();
        self.min = value.clamp(mode.range_min(), mode.range_max() - 1);
        if self.max <= self.min {
            self.max = self.min + 1;
        }
    }

    /// Sets max, pushing min down if they would cross.
    pub fn set_max(&mut self, value: i32) {
        let mode = self.mode();
        self.max = value.clamp(mode.range_min() + 1, mode.range_max());
        if self.min >= self.max {
            self.min = self.max - 1;
        }
    }

    pub fn set_param(&mut self, value: i32) {
        if let Some(spec) = self.mode().param() {
            self.param = value.clamp(spec.min, spec.max);
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Switches
// ------------------------------------------------------------------------------------------------

/// What a foot switch does when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SwitchMode {
    CcOnOff = 1,
    CcImpulse = 2,
    CcToggle = 3,
    CcUpDown = 4,
    PcImpulse = 5,
    PcToggle = 6,
    PcUpDown = 7,
    NoteToggle = 8,
    TapTempo = 9,
    StartStop = 10,
    ContinueStop = 11,
}

/// A 0..=127 value field of a switch mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueSpec {
    pub label: &'static str,
    pub default: i32,
}

impl SwitchMode {
    pub const ALL: [SwitchMode; 11] = [
        SwitchMode::CcOnOff,
        SwitchMode::CcImpulse,
        SwitchMode::CcToggle,
        SwitchMode::CcUpDown,
        SwitchMode::PcImpulse,
        SwitchMode::PcToggle,
        SwitchMode::PcUpDown,
        SwitchMode::NoteToggle,
        SwitchMode::TapTempo,
        SwitchMode::StartStop,
        SwitchMode::ContinueStop,
    ];

    pub fn from_code(code: u8) -> Option<SwitchMode> {
        SwitchMode::ALL.into_iter().find(|m| *m as u8 == code)
    }

    pub fn key(self) -> String {
        (self as u8).to_string()
    }

    pub fn from_key(key: &str) -> Option<SwitchMode> {
        key.parse().ok().and_then(SwitchMode::from_code)
    }

    pub fn label(self) -> &'static str {
        match self {
            SwitchMode::CcOnOff => "On / Off value",
            SwitchMode::CcImpulse | SwitchMode::PcImpulse => "Impulse",
            SwitchMode::CcToggle => "Toggle Controller #",
            SwitchMode::CcUpDown => "Up / Down Controller #",
            SwitchMode::PcToggle => "Toggle Program #",
            SwitchMode::PcUpDown => "Up / Down Program #",
            SwitchMode::NoteToggle => "Toggle Note #",
            SwitchMode::TapTempo => "Tap Tempo (Timing Clock)",
            SwitchMode::StartStop => "Start / Stop",
            SwitchMode::ContinueStop => "Continue / Stop",
        }
    }

    pub fn value1(self) -> Option<ValueSpec> {
        let (label, default) = match self {
            SwitchMode::CcOnOff => ("Off", 0),
            SwitchMode::CcImpulse | SwitchMode::CcToggle | SwitchMode::CcUpDown => ("Value", 127),
            SwitchMode::PcImpulse | SwitchMode::PcToggle | SwitchMode::PcUpDown => ("Bank MSB", 0),
            SwitchMode::NoteToggle => ("Velocity", 127),
            _ => return None,
        };
        Some(ValueSpec { label, default })
    }

    pub fn value2(self) -> Option<ValueSpec> {
        let (label, default) = match self {
            SwitchMode::CcOnOff => ("On", 127),
            SwitchMode::PcImpulse | SwitchMode::PcToggle | SwitchMode::PcUpDown => ("Bank LSB", 0),
            _ => return None,
        };
        Some(ValueSpec { label, default })
    }

    /// One sentence on what pressing the button does.
    pub fn hint(self) -> &'static str {
        match self {
            SwitchMode::CcOnOff => {
                "Sends the On value when pressed and the Off value when released (in Toggle \
                 button mode: on every second press)."
            }
            SwitchMode::CcImpulse => "Every press sends the controller with Value.",
            SwitchMode::CcToggle => "Presses alternate between Controller 1 and Controller 2.",
            SwitchMode::CcUpDown => "Every press moves the controller by Step (negative goes down).",
            SwitchMode::PcImpulse => "Every press selects the program, in the given bank.",
            SwitchMode::PcToggle => "Presses alternate between Program 1 and Program 2.",
            SwitchMode::PcUpDown => "Every press moves through programs by Step (negative goes down).",
            SwitchMode::NoteToggle => "Presses alternate between Note 1 and Note 2.",
            SwitchMode::TapTempo => "Tap repeatedly to set the MIDI clock tempo.",
            SwitchMode::StartStop => "Presses alternate between MIDI Start and Stop.",
            SwitchMode::ContinueStop => "Presses alternate between MIDI Continue and Stop.",
        }
    }

    /// Off must stay below On.
    pub fn values_ordered(self) -> bool {
        self == SwitchMode::CcOnOff
    }

    fn param_range(self) -> (i32, i32, ParamNames) {
        match self {
            SwitchMode::CcOnOff | SwitchMode::CcImpulse | SwitchMode::CcToggle | SwitchMode::CcUpDown => {
                (0, 119, ParamNames::Controller)
            }
            SwitchMode::PcImpulse | SwitchMode::PcToggle | SwitchMode::PcUpDown => (1, 128, ParamNames::None),
            SwitchMode::NoteToggle => (0, 127, ParamNames::Note),
            _ => (0, 1, ParamNames::None),
        }
    }

    pub fn param1(self) -> Option<ParamSpec> {
        let (min, max, names) = self.param_range();
        let (label, default) = match self {
            SwitchMode::CcOnOff | SwitchMode::CcImpulse | SwitchMode::CcUpDown => ("Controller #", 40),
            SwitchMode::CcToggle => ("Controller 1 #", 40),
            SwitchMode::PcImpulse | SwitchMode::PcUpDown => ("Program #", 1),
            SwitchMode::PcToggle => ("Program 1 #", 1),
            SwitchMode::NoteToggle => ("Note 1 #", 60),
            _ => return None,
        };
        Some(ParamSpec { label, min, max, default, names })
    }

    pub fn param2(self) -> Option<ParamSpec> {
        let (min, max, names) = self.param_range();
        let (label, default) = match self {
            SwitchMode::CcToggle => ("Controller 2 #", 41),
            SwitchMode::PcToggle => ("Program 2 #", 2),
            SwitchMode::NoteToggle => ("Note 2 #", 62),
            _ => return None,
        };
        Some(ParamSpec { label, min, max, default, names })
    }

    /// Up/down modes step through programs/values.
    pub fn has_step(self) -> bool {
        matches!(self, SwitchMode::CcUpDown | SwitchMode::PcUpDown)
    }

    /// Largest step magnitude.
    pub fn step_limit(self) -> i32 {
        self.param_range().1 - 1
    }

    /// Default for the param fields, used for modes without a visible parameter.
    fn param_defaults(self) -> (i32, i32) {
        (
            self.param1().map_or(0, |p| p.default),
            self.param2().map_or(match self {
                SwitchMode::PcImpulse | SwitchMode::PcUpDown => 1,
                _ => 0,
            }, |p| p.default),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ButtonType {
    Latching = 0,
    NonLatching = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ButtonMode {
    Momentary = 0,
    Toggle = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchConfig {
    pub message: MessageType,
    /// 1..=16
    pub channel: u8,
    pub mode: SwitchMode,
    pub value1: i32,
    pub value2: i32,
    /// Shown 1-based for program changes, like the original app.
    pub param1: i32,
    pub param2: i32,
    /// Signed step for up/down modes.
    pub step: i32,
    pub button_type: ButtonType,
    pub button_mode: ButtonMode,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        let mut switch = SwitchConfig {
            message: MessageType::ControlChange,
            channel: 1,
            mode: SwitchMode::CcOnOff,
            value1: 0,
            value2: 0,
            param1: 0,
            param2: 0,
            step: 1,
            button_type: ButtonType::Latching,
            button_mode: ButtonMode::Momentary,
        };
        switch.set_message(MessageType::ControlChange);
        switch
    }
}

impl SwitchConfig {
    /// Switches message type and selects its first mode.
    pub fn set_message(&mut self, message: MessageType) {
        self.message = message;
        if let Some(&first) = message.switch_modes().first() {
            self.set_mode(first);
        }
    }

    /// Switches mode and resets values to its defaults.
    pub fn set_mode(&mut self, mode: SwitchMode) {
        self.mode = mode;
        self.value1 = mode.value1().map_or(0, |v| v.default);
        self.value2 = mode.value2().map_or(0, |v| v.default);
        (self.param1, self.param2) = mode.param_defaults();
        self.step = 1;
    }

    pub fn set_value1(&mut self, value: i32) {
        if self.mode.values_ordered() {
            self.value1 = value.clamp(0, 126);
            if self.value2 <= self.value1 {
                self.value2 = self.value1 + 1;
            }
        } else {
            self.value1 = value.clamp(0, 127);
        }
    }

    pub fn set_value2(&mut self, value: i32) {
        if self.mode.values_ordered() {
            self.value2 = value.clamp(1, 127);
            if self.value1 >= self.value2 {
                self.value1 = self.value2 - 1;
            }
        } else {
            self.value2 = value.clamp(0, 127);
        }
    }

    pub fn set_param1(&mut self, value: i32) {
        if let Some(spec) = self.mode.param1() {
            self.param1 = value.clamp(spec.min, spec.max);
        }
    }

    pub fn set_param2(&mut self, value: i32) {
        if let Some(spec) = self.mode.param2() {
            self.param2 = value.clamp(spec.min, spec.max);
        }
    }

    /// Sets the step, wrapping around at the ends like the original spinner.
    pub fn set_step(&mut self, value: i32) {
        let limit = self.mode.step_limit().max(1);
        let span = 2 * limit + 1;
        self.step = (value + limit).rem_euclid(span) - limit;
    }

    /// The parameter as stored on the device (program changes are 0-based there).
    fn wire_param(&self, value: i32) -> u8 {
        let value = if self.message == MessageType::ProgramChange { value - 1 } else { value };
        (value & 0x7F) as u8
    }

    /// Copies everything the "link" ties together from `other`, mirroring its step.
    pub fn mirror(&mut self, other: &SwitchConfig) {
        self.message = other.message;
        self.channel = other.channel;
        self.mode = other.mode;
        self.value1 = other.value1;
        self.value2 = other.value2;
        self.param1 = other.param1;
        self.step = -other.step;
    }
}

/// Which of the two switch inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Switch {
    One,
    Two,
}

struct SwitchCommands {
    mode: Command,
    range: Command,
    message1_w: Command,
    message1_r: Command,
    step: Command,
    button_type: Command,
    button_mode: Command,
    message2_w: Command,
    message2_r: Command,
}

impl Switch {
    fn commands(self) -> SwitchCommands {
        match self {
            Switch::One => SwitchCommands {
                mode: Command::Switch1Mode,
                range: Command::Switch1Range,
                message1_w: Command::Switch1Message1Write,
                message1_r: Command::Switch1Message1Read,
                step: Command::Switch1Step,
                button_type: Command::Switch1ButtonType,
                button_mode: Command::Switch1ButtonMode,
                message2_w: Command::Switch1Message2Write,
                message2_r: Command::Switch1Message2Read,
            },
            Switch::Two => SwitchCommands {
                mode: Command::Switch2Mode,
                range: Command::Switch2Range,
                message1_w: Command::Switch2Message1Write,
                message1_r: Command::Switch2Message1Read,
                step: Command::Switch2Step,
                button_type: Command::Switch2ButtonType,
                button_mode: Command::Switch2ButtonMode,
                message2_w: Command::Switch2Message2Write,
                message2_r: Command::Switch2Message2Read,
            },
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Whole device configuration
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub pedal: PedalConfig,
    pub switch1: SwitchConfig,
    pub switch2: SwitchConfig,
    /// Button 2 mirrors button 1 (with the step reversed).
    pub linked: bool,
    /// Whether the pedal block is written. Off when the device reported a blank pedal, so an
    /// untouched pedal stays exactly as it was. Not stored on the device.
    pub pedal_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            pedal: PedalConfig::default(),
            switch1: SwitchConfig::default(),
            switch2: SwitchConfig::default(),
            linked: false,
            pedal_enabled: true,
        }
    }
}

impl Config {
    pub fn switch(&self, which: Switch) -> &SwitchConfig {
        match which {
            Switch::One => &self.switch1,
            Switch::Two => &self.switch2,
        }
    }

    pub fn switch_mut(&mut self, which: Switch) -> &mut SwitchConfig {
        match which {
            Switch::One => &mut self.switch1,
            Switch::Two => &mut self.switch2,
        }
    }

    /// Edits one switch and keeps the other in step when they are linked.
    pub fn edit_switch(&mut self, which: Switch, edit: impl FnOnce(&mut SwitchConfig)) {
        edit(self.switch_mut(which));
        if self.linked {
            self.sync_link(which);
        }
    }

    /// Turns linking on or off. Turning it on copies `from` into the other switch.
    pub fn set_linked(&mut self, linked: bool, from: Switch) {
        self.linked = linked;
        if linked {
            self.sync_link(from);
        }
    }

    fn sync_link(&mut self, master: Switch) {
        let source = self.switch(master).clone();
        let other = match master {
            Switch::One => Switch::Two,
            Switch::Two => Switch::One,
        };
        self.switch_mut(other).mirror(&source);
    }

    /// The writes that store this configuration, in the original app's order.
    pub fn write_messages(&self) -> Vec<Vec<u8>> {
        let pedal = &self.pedal;
        let divisor = pedal.message.range_divisor();
        let mut out = Vec::new();
        if self.pedal_enabled {
            out.extend([
            write(Command::PedalMode, &[pedal.mode() as u8]),
            write(
                Command::PedalRange,
                &[(pedal.min / divisor) as u8, (pedal.max / divisor) as u8],
            ),
            write(
                Command::PedalMessageWrite,
                &pedal.message.message_bytes(pedal.channel, (pedal.param & 0x7F) as u8),
            ),
            write(Command::PedalReversed, &[u8::from(pedal.reversed)]),
            ]);
        }
        for which in [Switch::One, Switch::Two] {
            let switch = self.switch(which);
            let commands = which.commands();
            out.push(write(commands.mode, &[switch.mode as u8]));
            out.push(write(
                commands.range,
                &[switch.value1 as u8, switch.value2 as u8],
            ));
            out.push(write(
                commands.message1_w,
                &switch
                    .message
                    .message_bytes(switch.channel, switch.wire_param(switch.param1)),
            ));
            if switch.mode.has_step() {
                out.push(write(
                    commands.step,
                    &[switch.step.unsigned_abs() as u8, u8::from(switch.step < 0)],
                ));
            }
            out.push(write(commands.button_type, &[switch.button_type as u8]));
            out.push(write(commands.button_mode, &[switch.button_mode as u8]));
            if switch.mode.param2().is_some() {
                out.push(write(
                    commands.message2_w,
                    &switch
                        .message
                        .message_bytes(switch.channel, switch.wire_param(switch.param2)),
                ));
            }
        }
        out.push(write(Command::SwitchesLinked, &[u8::from(self.linked)]));
        out
    }
}

/// The reads that fetch a whole configuration, with the number of value bytes each returns.
pub fn read_requests() -> Vec<(Command, u8)> {
    let mut out = vec![
        (Command::Info, 3),
        (Command::PedalMode, 1),
        (Command::PedalMessageRead, 2),
        (Command::PedalRange, 2),
        (Command::PedalReversed, 1),
    ];
    for which in [Switch::One, Switch::Two] {
        let c = which.commands();
        out.extend([
            (c.mode, 1),
            (c.range, 2),
            (c.message1_r, 2),
            (c.step, 2),
            (c.button_type, 1),
            (c.button_mode, 1),
            (c.message2_r, 2),
        ]);
    }
    out.push((Command::SwitchesLinked, 1));
    out
}

/// Assembles a [`Config`] from device responses.
#[derive(Debug, Default)]
pub struct ConfigReader {
    pub info: Option<DeviceInfo>,
    config: Config,
    pedal_range: Option<(u8, u8)>,
    pedal_message: Option<MessageType>,
    switch_mode: [Option<SwitchMode>; 2],
    switch_message: [Option<MessageType>; 2],
    switch_param2: [Option<u8>; 2],
}

impl ConfigReader {
    /// Feeds one response. Returns false when the frame is not one we understand.
    pub fn apply(&mut self, command: u8, values: &[u8]) -> bool {
        let Some(command) = Command::from_code(command) else {
            return false;
        };
        let at = |i: usize| values.get(i).copied();
        match command {
            Command::Info => self.info = Some(DeviceInfo::from_values(values)),
            // Derived from the message type, which is read separately.
            Command::PedalMode => {}
            Command::PedalMessageRead => {
                let Some(status) = at(0) else { return false };
                let (status, channel) = decode_status(status);
                let Some(message) = MessageType::from_status(status) else {
                    return false;
                };
                if !MessageType::PEDAL.contains(&message) {
                    return false;
                }
                self.pedal_message = Some(message);
                self.config.pedal.message = message;
                self.config.pedal.channel = channel;
                if let Some(param) = at(1) {
                    self.config.pedal.param = i32::from(param);
                }
            }
            Command::PedalRange => {
                let (Some(min), Some(max)) = (at(0), at(1)) else { return false };
                self.pedal_range = Some((min, max));
            }
            Command::PedalReversed => self.config.pedal.reversed = at(0).unwrap_or(0) != 0,
            Command::SwitchesLinked => self.config.linked = at(0).unwrap_or(0) != 0,
            Command::Switch1Mode | Command::Switch2Mode => {
                let which = if command == Command::Switch1Mode { 0 } else { 1 };
                self.switch_mode[which] = at(0).and_then(SwitchMode::from_code);
            }
            Command::Switch1Range | Command::Switch2Range => {
                let switch = self.switch_for(command == Command::Switch1Range);
                switch.value1 = i32::from(at(0).unwrap_or(0));
                switch.value2 = i32::from(at(1).unwrap_or(0));
            }
            Command::Switch1Message1Read | Command::Switch2Message1Read => {
                let first = command == Command::Switch1Message1Read;
                let Some(status) = at(0) else { return false };
                let (status, channel) = decode_status(status);
                let Some(message) = MessageType::from_status(status) else {
                    return false;
                };
                self.switch_message[usize::from(!first)] = Some(message);
                let switch = self.switch_for(first);
                switch.message = message;
                if message.has_channel() {
                    switch.channel = channel;
                    if let Some(param) = at(1) {
                        switch.param1 = i32::from(param)
                            + i32::from(message == MessageType::ProgramChange);
                    }
                }
            }
            Command::Switch1Message2Read | Command::Switch2Message2Read => {
                let first = command == Command::Switch1Message2Read;
                // Program changes are shifted to 1-based in `finish`, once the type is known.
                self.switch_param2[usize::from(!first)] = at(1);
            }
            Command::Switch1Step | Command::Switch2Step => {
                let switch = self.switch_for(command == Command::Switch1Step);
                let magnitude = i32::from(at(0).unwrap_or(1));
                switch.step = if at(1).unwrap_or(0) != 0 { -magnitude } else { magnitude };
            }
            Command::Switch1ButtonType | Command::Switch2ButtonType => {
                self.switch_for(command == Command::Switch1ButtonType).button_type =
                    if at(0) == Some(1) { ButtonType::NonLatching } else { ButtonType::Latching };
            }
            Command::Switch1ButtonMode | Command::Switch2ButtonMode => {
                self.switch_for(command == Command::Switch1ButtonMode).button_mode =
                    if at(0) == Some(1) { ButtonMode::Toggle } else { ButtonMode::Momentary };
            }
            _ => return false,
        }
        true
    }

    fn switch_for(&mut self, first: bool) -> &mut SwitchConfig {
        if first { &mut self.config.switch1 } else { &mut self.config.switch2 }
    }

    /// The assembled configuration. Missing or inconsistent fields fall back to defaults.
    pub fn finish(mut self) -> Config {
        self.config.pedal_enabled = self.pedal_message.is_some();
        let pedal = &mut self.config.pedal;
        if self.pedal_message.is_some() {
            if let Some((min, max)) = self.pedal_range {
                let divisor = pedal.message.range_divisor();
                pedal.min = i32::from(min) * divisor;
                pedal.max = i32::from(max) * divisor;
            }
            if let Some(spec) = pedal.mode().param() {
                pedal.param = pedal.param.clamp(spec.min, spec.max);
            }
        }
        for (index, switch) in [&mut self.config.switch1, &mut self.config.switch2]
            .into_iter()
            .enumerate()
        {
            if let Some(param) = self.switch_param2[index] {
                switch.param2 =
                    i32::from(param) + i32::from(switch.message == MessageType::ProgramChange);
            }
            match self.switch_mode[index] {
                Some(mode) if switch.message.switch_modes().contains(&mode) => switch.mode = mode,
                // The device said nothing usable: show the message type's first mode, but keep
                // whatever values it did report.
                _ => {
                    if let Some(&first) = switch.message.switch_modes().first() {
                        switch.mode = first;
                    }
                }
            }
        }
        self.config
    }
}

// ------------------------------------------------------------------------------------------------
// Names
// ------------------------------------------------------------------------------------------------

/// The General MIDI name of a controller number, or "" when it has none.
pub fn cc_name(cc: i32) -> &'static str {
    match cc {
        0 => "Bank Select",
        1 => "Modulation Wheel",
        2 => "Breath Controller",
        4 => "Foot Controller",
        5 => "Portamento Time",
        6 => "Data Entry MSB",
        7 => "Channel Volume",
        8 => "Balance",
        10 => "Pan",
        11 => "Expression Controller",
        12 => "Effect 1",
        13 => "Effect 2",
        16 | 80 => "General Purpose 1",
        17 | 81 => "General Purpose 2",
        18 | 82 => "General Purpose 3",
        19 | 83 => "General Purpose 4",
        64 => "Sustain on/off",
        65 => "Portamento on/off",
        66 => "Sostenuto on/off",
        67 => "Soft Pedal on/off",
        68 => "Legato Footswitch",
        69 => "Hold 2",
        70 => "Sound Variation",
        71 => "Timbre",
        72 => "Release Time",
        73 => "Attack Time",
        74 => "Brightness",
        75 => "Decay Time",
        76 => "Vibrato Rate",
        77 => "Vibrato Depth",
        78 => "Vibrato Delay",
        79 => "Sound Controller",
        84 => "Portamento",
        88 => "Hi-Res Velocity Prefix",
        91 => "Effects 1 Depth",
        92 => "Effects 2 Depth",
        93 => "Effects 3 Depth",
        94 => "Effects 4 Depth",
        95 => "Effects 5 Depth",
        96 => "Data Entry +1",
        97 => "Data Entry -1",
        _ => "",
    }
}

/// Note name with octave (C4 = 60), sharps only.
pub fn note_name(note: i32) -> &'static str {
    const NAMES: [&str; 128] = {
        // Built by hand so the strings can be 'static without allocation.
        [
            "C-1", "C#-1", "D-1", "D#-1", "E-1", "F-1", "F#-1", "G-1", "G#-1", "A-1", "A#-1", "B-1",
            "C0", "C#0", "D0", "D#0", "E0", "F0", "F#0", "G0", "G#0", "A0", "A#0", "B0",
            "C1", "C#1", "D1", "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1",
            "C2", "C#2", "D2", "D#2", "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2",
            "C3", "C#3", "D3", "D#3", "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3",
            "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4", "A#4", "B4",
            "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5", "G#5", "A5", "A#5", "B5",
            "C6", "C#6", "D6", "D#6", "E6", "F6", "F#6", "G6", "G#6", "A6", "A#6", "B6",
            "C7", "C#7", "D7", "D#7", "E7", "F7", "F#7", "G7", "G#7", "A7", "A#7", "B7",
            "C8", "C#8", "D8", "D#8", "E8", "F8", "F#8", "G8", "G#8", "A8", "A#8", "B8",
            "C9", "C#9", "D9", "D#9", "E9", "F9", "F#9", "G9",
        ]
    };
    usize::try_from(note).ok().and_then(|n| NAMES.get(n)).copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_the_original_app() {
        // SysExUtils.prepareMessage(INFO, READ, 3)
        assert_eq!(read(Command::Info, 3), [0xF0, 0x00, 0x21, 0x3A, 0x01, 0x03, 0x01, 0xF7]);
        // prepareMessage(PEDAL_REVERSED, WRITE, 1)
        assert_eq!(
            write(Command::PedalReversed, &[1]),
            [0xF0, 0x00, 0x21, 0x3A, 0x10, 0x01, 0x02, 0x01, 0xF7]
        );
        assert_eq!(
            execute(Command::FactoryReset),
            [0xF0, 0x00, 0x21, 0x3A, 0x7F, 0x00, 0x03, 0xF7]
        );
    }

    #[test]
    fn status_bytes_round_trip() {
        // toStatusByte(0xB0, channel 0) = 0xB0 - 127 = 0x31
        assert_eq!(encode_status(0xB0, 1), 0x31);
        assert_eq!(encode_status(0xC0, 16), (0xCF - 127) as u8);
        assert_eq!(encode_status(0xF8, 1), 0xF8 - 127);
        for message in MessageType::PEDAL.into_iter().chain(MessageType::SWITCH) {
            for channel in 1..=16 {
                let byte = encode_status(message.status(), channel);
                assert!(byte < 0x80);
                let (status, decoded) = decode_status(byte);
                assert_eq!(status, message.status());
                if message.has_channel() {
                    assert_eq!(decoded, channel);
                }
            }
        }
    }

    #[test]
    fn splits_responses() {
        let raw = [0xF0, 0x00, 0x21, 0x3A, 0x01, 0x03, 0x01, 0x03, 0xF7];
        assert_eq!(split_response(&raw), Some((1, &[3, 1, 3][..])));
        assert_eq!(split_response(&[0xF0, 0x7E, 0x00, 0x06, 0x02, 0xF7]), None);
        assert_eq!(split_response(&[0x90, 60, 100]), None);
    }

    /// Simulates a device that stores the writes and answers the reads in the app's order.
    fn respond(config: &Config, info: DeviceInfo) -> ConfigReader {
        let mut stored = std::collections::HashMap::new();
        for msg in config.write_messages() {
            stored.insert(msg[4], msg[7..msg.len() - 1].to_vec());
        }
        let firmware = info.firmware.unwrap();
        let kind = match info.device_type {
            Some(DeviceType::DualAdapter) => 3,
            _ => 7,
        };
        stored.insert(Command::Info as u8, vec![kind, firmware.major, firmware.minor]);
        let write_of = |read: Command| match read {
            Command::PedalMessageRead => Command::PedalMessageWrite,
            Command::Switch1Message1Read => Command::Switch1Message1Write,
            Command::Switch1Message2Read => Command::Switch1Message2Write,
            Command::Switch2Message1Read => Command::Switch2Message1Write,
            Command::Switch2Message2Read => Command::Switch2Message2Write,
            other => other,
        };
        let mut reader = ConfigReader::default();
        for (command, _) in read_requests() {
            let written = write_of(command);
            let Some(values) = stored.get(&(written as u8)) else { continue };
            // Message reads answer with [status, param] (bytes 1 and 2 of the write).
            let reply = if written == command { values.clone() } else { values[1..3].to_vec() };
            assert!(reader.apply(command as u8, &reply), "{command:?}");
        }
        reader
    }

    #[test]
    fn write_then_read_round_trips() {
        let mut config = Config::default();
        config.pedal.set_message(MessageType::Clock);
        config.pedal.set_min(40);
        config.pedal.set_max(200);
        config.pedal.reversed = true;
        config.switch1.set_message(MessageType::ProgramChange);
        config.switch1.set_mode(SwitchMode::PcToggle);
        config.switch1.channel = 5;
        config.switch1.set_param1(128);
        config.switch1.set_param2(1);
        config.switch1.button_type = ButtonType::NonLatching;
        config.switch1.button_mode = ButtonMode::Toggle;
        config.switch2.set_mode(SwitchMode::CcUpDown);
        config.switch2.set_step(-3);
        config.switch2.channel = 16;

        let info = DeviceInfo {
            device_type: Some(DeviceType::DualAdapter),
            firmware: Some(Firmware::new(1, 3)),
        };
        let reader = respond(&config, info);
        assert_eq!(reader.info, Some(info));
        assert_eq!(reader.finish(), config);
    }

    #[test]
    fn write_order_matches_the_original_app() {
        let config = Config::default();
        let commands: Vec<u8> = config.write_messages().iter().map(|m| m[4]).collect();
        // Default switches are CC on/off: no step, no second message.
        assert_eq!(commands, [8, 9, 11, 16, 28, 29, 31, 38, 39, 48, 49, 51, 58, 59, 70]);
    }

    #[test]
    fn pedal_clock_range_is_halved_on_the_wire() {
        let mut config = Config::default();
        config.pedal.set_message(MessageType::Clock);
        assert_eq!((config.pedal.min, config.pedal.max), (10, 254));
        let range = &config.write_messages()[1];
        assert_eq!(&range[7..9], &[5, 127]);
    }

    #[test]
    fn program_change_is_one_based_in_the_ui() {
        let mut switch = SwitchConfig::default();
        switch.set_message(MessageType::ProgramChange);
        switch.set_param1(1);
        let config = Config { switch1: switch, ..Config::default() };
        let msg = config
            .write_messages()
            .into_iter()
            .find(|m| m[4] == Command::Switch1Message1Write as u8)
            .unwrap();
        assert_eq!(msg[7..12], [12, encode_status(0xC0, 1), 0, 0, 1]);
    }

    #[test]
    fn min_and_max_never_cross() {
        let mut pedal = PedalConfig::default();
        pedal.set_max(10);
        pedal.set_min(10);
        assert_eq!((pedal.min, pedal.max), (10, 11));
        pedal.set_max(5);
        assert_eq!((pedal.min, pedal.max), (4, 5));

        let mut switch = SwitchConfig::default();
        assert_eq!(switch.mode, SwitchMode::CcOnOff);
        switch.set_value2(0);
        assert_eq!((switch.value1, switch.value2), (0, 1));
    }

    #[test]
    fn step_wraps_around() {
        let mut switch = SwitchConfig::default();
        switch.set_mode(SwitchMode::CcUpDown);
        switch.set_step(118);
        assert_eq!(switch.step, 118);
        switch.set_step(119);
        assert_eq!(switch.step, -118);
        switch.set_step(-119);
        assert_eq!(switch.step, 118);
    }

    #[test]
    fn linking_mirrors_with_reversed_step() {
        let mut config = Config::default();
        config.edit_switch(Switch::One, |s| s.set_mode(SwitchMode::CcUpDown));
        config.set_linked(true, Switch::One);
        config.edit_switch(Switch::One, |s| s.set_step(4));
        assert_eq!(config.switch2.mode, SwitchMode::CcUpDown);
        assert_eq!(config.switch2.step, -4);
        config.edit_switch(Switch::Two, |s| s.channel = 9);
        assert_eq!(config.switch1.channel, 9);
        assert_eq!(config.switch1.step, 4);
    }

    #[test]
    fn firmware_gates_choices() {
        let old = Firmware::new(1, 0);
        assert!(!MessageType::switch_choices(old).contains(&MessageType::NoteOn));
        assert!(!MessageType::pedal_choices(old).contains(&MessageType::Clock));
        let new = Firmware::new(1, 3);
        assert_eq!(MessageType::switch_choices(new).len(), 4);
        assert!(new.has_button_type());
    }

    /// Verbatim answers of a real "beat bars controller" (two buttons, no pedal configured).
    #[test]
    fn reads_a_real_controller() {
        let answers: [(u8, &[u8]); 20] = [
            (0x01, &[0x00]),
            (0x08, &[0x02]),
            (0x0C, &[0x01, 0x00]),
            (0x09, &[0x00, 0x7F]),
            (0x10, &[0x00]),
            (0x1C, &[0x01]),
            (0x1D, &[0x00, 0x7F]),
            (0x20, &[0x32, 0x50]),
            (0x24, &[0x01, 0x00]),
            (0x26, &[0x01]),
            (0x27, &[0x01]),
            (0x2A, &[0x32, 0x00]),
            (0x30, &[0x01]),
            (0x31, &[0x00, 0x7F]),
            (0x34, &[0x32, 0x51]),
            (0x38, &[0x01, 0x00]),
            (0x3A, &[0x01]),
            (0x3B, &[0x01]),
            (0x3E, &[0x32, 0x00]),
            (0x46, &[0x00]),
        ];
        let mut reader = ConfigReader::default();
        for (command, values) in answers {
            reader.apply(command, values);
        }
        let info = reader.info.unwrap();
        assert_eq!(info, DeviceInfo { device_type: None, firmware: None });
        assert!(info.has_switches() && info.has_button_type());
        let config = reader.finish();
        assert!(!config.pedal_enabled);
        let expected = SwitchConfig {
            message: MessageType::ControlChange,
            channel: 2,
            mode: SwitchMode::CcOnOff,
            value1: 0,
            value2: 127,
            param1: 80,
            param2: 0,
            step: 1,
            button_type: ButtonType::NonLatching,
            button_mode: ButtonMode::Toggle,
        };
        assert_eq!(config.switch1, expected);
        assert_eq!(config.switch2, SwitchConfig { param1: 81, ..expected });
        // Nothing about the blank pedal is written back.
        assert!(config.write_messages().iter().all(|m| m[4] >= 28));
    }

    #[test]
    fn names() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(127), "G9");
        assert_eq!(cc_name(11), "Expression Controller");
        assert_eq!(cc_name(3), "");
    }
}
