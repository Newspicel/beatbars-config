//! Application state and the device operations behind the buttons.
//!
//! Every exchange with the hardware blocks for a while, so it runs on a worker through
//! [`blocking`] and comes back to the UI thread at the `await`.

use std::sync::Arc;
use std::time::Duration;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui_ui::prelude::*;

use crate::midi::{self, Device};
use crate::protocol::{Config, DeviceInfo, Switch, SwitchConfig, PedalConfig};

/// How often the app looks for the device being plugged in or pulled out.
pub const SCAN_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq)]
pub enum Connection {
    /// Nothing tried yet.
    Searching,
    /// No device, with the reason if something went wrong.
    Absent(Option<String>),
    Connected { name: String, info: DeviceInfo },
}

/// Everything the views share. All handles, so it is `Copy`.
#[derive(Clone, Copy)]
pub struct Ctx {
    /// What is being edited.
    pub config: RwSignal<Config, LocalStorage>,
    /// What the device holds, as last read.
    pub saved: RwSignal<Config, LocalStorage>,
    pub connection: RwSignal<Connection, LocalStorage>,
    /// What the device is busy with, if anything.
    pub busy: RwSignal<Option<&'static str>, LocalStorage>,
    pub device: RwSignal<Option<Arc<Device>>, LocalStorage>,
    /// Which tab shows.
    pub tab: RwSignal<String, LocalStorage>,
    /// Pedal calibration progress, 0..=100, while it runs.
    pub calibration: RwSignal<Option<f64>, LocalStorage>,
    toasts: Option<ToastQueue>,
}

impl Ctx {
    pub fn new(toasts: Option<ToastQueue>) -> Ctx {
        Ctx {
            config: RwSignal::new_local(Config::default()),
            saved: RwSignal::new_local(Config::default()),
            connection: RwSignal::new_local(Connection::Searching),
            busy: RwSignal::new_local(None),
            device: RwSignal::new_local(None),
            tab: RwSignal::new_local("button1".to_owned()),
            calibration: RwSignal::new_local(None),
            toasts,
        }
    }

    /// The connected device's identity (tracked). Defaults to "unknown", which enables everything.
    pub fn info(&self) -> DeviceInfo {
        self.connection.with(|c| match c {
            Connection::Connected { info, .. } => *info,
            _ => DeviceInfo::default(),
        })
    }

    /// Whether to offer the pedal (tracked). A device that does not say what it is gets the pedal
    /// only if its pedal block holds a real configuration; a two-button controller's does not.
    pub fn has_pedal(&self) -> bool {
        let info = self.info();
        match info.device_type {
            Some(kind) => kind.has_pedal(),
            None => self.saved.with(|c| c.pedal_enabled),
        }
    }

    pub fn connected(&self) -> bool {
        self.connection.with(|c| matches!(c, Connection::Connected { .. }))
    }

    /// Whether the editor differs from what the device holds (tracked).
    pub fn dirty(&self) -> bool {
        self.config
            .with(|c| self.saved.with(|s| c.write_messages() != s.write_messages()))
    }

    pub fn edit_pedal(&self, edit: impl FnOnce(&mut PedalConfig)) {
        self.config.update(|c| {
            edit(&mut c.pedal);
            c.pedal_enabled = true;
        });
    }

    pub fn edit_switch(&self, which: Switch, edit: impl FnOnce(&mut SwitchConfig)) {
        self.config.update(|c| c.edit_switch(which, edit));
    }

    fn toast(&self, toast: Toast) {
        if let Some(toasts) = self.toasts {
            toasts.push(toast);
        }
    }

    fn fail(&self, error: &midi::Error) {
        self.device.set(None);
        self.connection.set(Connection::Absent(Some(error.to_string())));
        self.toast(
            Toast::new("Lost the device")
                .description(error.to_string())
                .kind(ToastKind::Error),
        );
    }

    /// Takes a freshly read configuration as both the editor's and the device's state.
    fn loaded(&self, info: DeviceInfo, config: Config) {
        let name = self.device.with_untracked(|d| d.as_ref().map(|d| d.name.clone()));
        self.connection.set(Connection::Connected {
            name: name.unwrap_or_default(),
            info,
        });
        self.saved.set(config.clone());
        self.config.set(config);
    }

    /// Opens the device and reads it. `quiet` keeps a failure from raising a toast.
    pub fn connect(self, quiet: bool) {
        if self.busy.get_untracked().is_some() {
            return;
        }
        self.busy.set(Some("Looking for the device…"));
        spawn_local(async move {
            let result = blocking(|| -> midi::Result<_> {
                let device = Device::open()?;
                let (info, config) = device.read_config()?;
                Ok((Arc::new(device), info, config))
            })
            .await;
            self.busy.set(None);
            match result {
                Ok((device, info, config)) => {
                    self.device.set(Some(device));
                    let pedal_first = config.pedal_enabled
                        && info.device_type.is_none_or(|kind| kind.has_pedal());
                    self.tab.set(if pedal_first { "pedal" } else { "button1" }.to_owned());
                    self.loaded(info, config);
                }
                Err(error) => {
                    self.device.set(None);
                    self.connection.set(Connection::Absent(match &error {
                        midi::Error::NotFound => None,
                        other => Some(other.to_string()),
                    }));
                    if !quiet {
                        self.toast(
                            Toast::new("No device")
                                .description(error.to_string())
                                .kind(ToastKind::Warning),
                        );
                    }
                }
            }
        });
    }

    /// Runs `job` against the device on a worker, showing `label` meanwhile.
    fn run<T: Send + 'static>(
        self,
        label: &'static str,
        job: impl FnOnce(&Device) -> midi::Result<T> + Send + 'static,
        done: impl FnOnce(Ctx, T) + 'static,
    ) {
        if self.busy.get_untracked().is_some() {
            return;
        }
        let Some(device) = self.device.get_untracked() else {
            self.connect(false);
            return;
        };
        self.busy.set(Some(label));
        spawn_local(async move {
            let result = blocking(move || job(&device)).await;
            self.busy.set(None);
            match result {
                Ok(value) => done(self, value),
                Err(error) => self.fail(&error),
            }
        });
    }

    /// Re-reads the device, dropping unsaved edits.
    pub fn reload(self) {
        self.run("Reading settings…", Device::read_config, |ctx, (info, config)| {
            ctx.loaded(info, config);
        });
    }

    /// Writes the editor to the device and reads it back to verify.
    pub fn write(self) {
        let config = self.config.get_untracked();
        self.run(
            "Writing settings…",
            move |device| {
                device.write_config(&config)?;
                let (info, stored) = device.read_config()?;
                Ok((config, info, stored))
            },
            |ctx, (written, info, stored)| {
                let verified = written.write_messages() == stored.write_messages();
                ctx.loaded(info, stored);
                if verified {
                    ctx.toast(Toast::new("Settings written").kind(ToastKind::Success));
                } else {
                    ctx.toast(
                        Toast::new("The device stored something else")
                            .description("Read back differs from what was written. The editor now shows what the device holds.")
                            .kind(ToastKind::Warning),
                    );
                }
            },
        );
    }

    pub fn factory_reset(self) {
        self.run(
            "Restoring factory settings…",
            |device| {
                device.factory_reset()?;
                device.read_config()
            },
            |ctx, (info, config)| {
                ctx.loaded(info, config);
                ctx.toast(Toast::new("Factory settings restored").kind(ToastKind::Success));
            },
        );
    }

    pub fn calibrate(self) {
        if self.busy.get_untracked().is_some() {
            return;
        }
        let Some(device) = self.device.get_untracked() else { return };
        self.busy.set(Some("Calibrating…"));
        self.calibration.set(Some(0.0));
        let started = std::time::Instant::now();
        let ticker = set_interval(Duration::from_millis(50), move || {
            let share = started.elapsed().as_secs_f64() / crate::protocol::CALIBRATION_TIME.as_secs_f64();
            self.calibration.set(Some((share * 100.0).min(100.0)));
        });
        spawn_local(async move {
            let result = blocking(move || device.calibrate()).await;
            ticker.cancel();
            self.calibration.set(None);
            self.busy.set(None);
            match result {
                Ok(true) => self.toast(Toast::new("Calibration finished").kind(ToastKind::Success)),
                Ok(false) => self.toast(
                    Toast::new("Calibration ended")
                        .description("The device did not confirm. Try again if the pedal range feels off.")
                        .kind(ToastKind::Warning),
                ),
                Err(error) => self.fail(&error),
            }
        });
    }

    /// Notices the device being plugged in or pulled out.
    pub fn watch(self) {
        let handle = set_interval(SCAN_INTERVAL, move || {
            if self.busy.get_untracked().is_some() {
                return;
            }
            spawn_local(async move {
                let present = blocking(midi::device_present).await;
                if self.busy.get_untracked().is_some() {
                    return;
                }
                let connected = self.device.with_untracked(Option::is_some);
                if present && !connected {
                    self.connect(true);
                } else if !present && connected {
                    self.device.set(None);
                    self.connection.set(Connection::Absent(None));
                    self.toast(Toast::new("Device unplugged").kind(ToastKind::Warning));
                }
            });
        });
        // The app root lives as long as the window; so does the watch.
        std::mem::forget(handle);
    }
}
