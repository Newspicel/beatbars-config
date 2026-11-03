//! The window: a header with the connection state, a tab per control and the actions.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_primitives::Orientation;
use zgui_ui_tokens::prelude::*;

use crate::app::{Connection, Ctx};
use crate::protocol::{
    ButtonMode, ButtonType, MessageType, ParamNames, Switch, SwitchMode, fine_pitch,
};

/// The whole window.
#[component]
pub fn App() -> impl IntoView {
    view! {
        ThemeProvider {
            Toaster(corner = ToastCorner::BottomRight, limit = 3) {
                Main()
            }
        }
    }
}

#[component]
fn Main() -> impl IntoView {
    let ctx = Ctx::new(use_toaster());
    ctx.connect(true);
    ctx.watch();

    view! {
        column(class = "app") {
            Header(ctx = ctx)
            if move || ctx.connected() {
                Editor(ctx = ctx)
            } else {
                NoDevice(ctx = ctx)
            }
            Footer(ctx = ctx)
            Show(when = move || ctx.busy.get().is_some()) {
                column(class = "busy") {
                    Spinner()
                    text(class = "busy__label") {{move || ctx.busy.get().unwrap_or_default()}}
                    Show(when = move || ctx.calibration.get().is_some()) {
                        column(class = "busy__calibration") {
                            text(class = "muted") {
                                "Move the pedal back and forth to both end positions, several times."
                            }
                            Progress(
                                value = Signal::derive_local(move || ctx.calibration.get()),
                                label = "Calibration"
                            )
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Header(ctx: Ctx) -> impl IntoView {
    let status = move || {
        ctx.connection.with(|c| match c {
            Connection::Searching => "Looking for a beat bars device…".to_owned(),
            Connection::Absent(_) => "No device connected".to_owned(),
            Connection::Connected { name, info } => format!("{name} · {}", info.summary()),
        })
    };
    view! {
        row(class = "header") {
            column(class = "header__text") {
                text(class = "title") {"beat bars config"}
                text(class = "muted") {{status}}
            }
            spacer()
            if move || ctx.connected() {
                Badge(variant = BadgeVariant::Default) {"Connected"}
            } else {
                Badge(variant = BadgeVariant::Outline) {"Offline"}
            }
        }
    }
}

#[component]
fn NoDevice(ctx: Ctx) -> impl IntoView {
    let reason = move || {
        ctx.connection.with(|c| match c {
            Connection::Absent(Some(reason)) => reason.clone(),
            _ => "Plug in a beat bars pedal, switch or adapter over USB. It is picked up \
                  automatically."
                .to_owned(),
        })
    };
    view! {
        column(class = "empty") {
            text(class = "empty__title") {"No device found"}
            text(class = "muted empty__text") {{reason}}
            Button(on:click = move |_| ctx.connect(false)) {"Scan USB"}
        }
    }
}

#[component]
fn Editor(ctx: Ctx) -> impl IntoView {
    let no_switches = Signal::derive_local(move || !ctx.info().has_switches());
    view! {
        Tabs(value = ctx.tab, label = "Controls", class = "tabs") {
            TabsList {
                Show(when = move || ctx.has_pedal()) {
                    TabsTrigger(value = "pedal") {"Pedal"}
                }
                TabsTrigger(value = "button1", disabled = no_switches) {"Button 1"}
                TabsTrigger(value = "button2", disabled = no_switches) {"Button 2"}
            }
            TabsContent(value = "pedal") {PedalPanel(ctx = ctx)}
            TabsContent(value = "button1") {SwitchPanel(ctx = ctx, which = Switch::One)}
            TabsContent(value = "button2") {SwitchPanel(ctx = ctx, which = Switch::Two)}
        }
    }
}

#[component]
fn Footer(ctx: Ctx) -> impl IntoView {
    let offline = Signal::derive_local(move || !ctx.connected() || ctx.busy.get().is_some());
    view! {
        row(class = "footer") {
            Show(when = move || ctx.connected()) {
                AlertDialog {
                    AlertDialogTrigger(variant = ButtonVariant::Outline) {
                        "Factory reset…"
                    }
                    AlertDialogContent {
                        AlertDialogHeader {
                            AlertDialogTitle {"Restore factory settings?"}
                            AlertDialogDescription {
                                "Every setting on the device goes back to how it shipped. This cannot be undone."
                            }
                        }
                        AlertDialogFooter {
                            AlertDialogCancel {"Cancel"}
                            AlertDialogAction(
                                variant = ButtonVariant::Destructive,
                                on:click = move |_| ctx.factory_reset()
                            ) {"Reset"}
                        }
                    }
                }
            }
            spacer()
            Show(when = move || ctx.connected() && ctx.dirty()) {
                text(class = "muted") {"Unsaved changes"}
            }
            Button(
                variant = ButtonVariant::Outline,
                disabled = offline,
                on:click = move |_| ctx.reload()
            ) {"Reload"}
            Button(disabled = offline, on:click = move |_| ctx.write()) {"Write to device"}
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Pedal
// ------------------------------------------------------------------------------------------------

#[component]
fn PedalPanel(ctx: Ctx) -> impl IntoView {
    let pedal = move || ctx.config.with(|c| c.pedal.clone());
    let mode = move || ctx.config.with(|c| c.pedal.mode());

    let message = Binding::controlled(
        Signal::derive_local(move || pedal().message.key().to_owned()),
        move |key: String| {
            if let Some(message) = MessageType::from_key(&key) {
                ctx.edit_pedal(|p| p.set_message(message));
            }
        },
    );
    let reversed = Binding::controlled(
        Signal::derive_local(move || pedal().reversed),
        move |on: bool| ctx.edit_pedal(|p| p.reversed = on),
    );

    view! {
        column(class = "panel") {
            Show(when = move || !ctx.config.with(|c| c.pedal_enabled)) {
                Alert {
                    AlertTitle {"The pedal input is not set up"}
                    AlertDescription {
                        "Nothing here is written to the device until you change a pedal setting."
                    }
                }
            }
            FormRow(label = "MIDI message") {
                Select(value = message) {
                    SelectTrigger(label = "MIDI message", class = "wide") {SelectValue()}
                    SelectContent {
                        for choice in move || MessageType::pedal_choices(ctx.info().firmware_or_latest()),
                            key = |m: &MessageType| *m
                        {
                            SelectItem(value = choice.key()) {{choice.label()}}
                        }
                    }
                }
            }
            FormRow(label = "MIDI channel") {
                NumberField(
                    label = "MIDI channel",
                    value = Signal::derive_local(move || i32::from(pedal().channel)),
                    min = 1,
                    max = 16,
                    disabled = Signal::derive_local(move || !pedal().message.has_channel()),
                    on_set = UnsyncCallback::new(move |v: i32| {
                        ctx.edit_pedal(|p| p.channel = v as u8)
                    })
                )
            }
            Show(when = move || mode().param().is_some()) {
                FormRow(label = Signal::derive_local(move || {
                    mode().param().map_or("", |p| p.label).to_owned()
                })) {
                    NumberField(
                        label = "Parameter",
                        value = Signal::derive_local(move || pedal().param),
                        min = Signal::derive_local(move || mode().param().map_or(0, |p| p.min)),
                        max = Signal::derive_local(move || mode().param().map_or(127, |p| p.max)),
                        on_set = UnsyncCallback::new(move |v: i32| ctx.edit_pedal(|p| p.set_param(v)))
                    )
                    text(class = "muted name") {{move || {
                        let names = mode().param().map_or(ParamNames::None, |p| p.names);
                        names.name(pedal().param).to_owned()
                    }}}
                }
            }
            FormRow(label = Signal::derive_local(move || mode().min_label().to_owned())) {
                NumberField(
                    label = "Minimum",
                    value = Signal::derive_local(move || pedal().min),
                    min = Signal::derive_local(move || mode().range_min()),
                    max = Signal::derive_local(move || mode().range_max() - 1),
                    on_set = UnsyncCallback::new(move |v: i32| ctx.edit_pedal(|p| p.set_min(v)))
                )
                Show(when = move || pedal().message.is_fine()) {
                    text(class = "muted name") {{move || format!("fine {}", fine_pitch(pedal().min, false))}}
                }
            }
            FormRow(label = Signal::derive_local(move || mode().max_label().to_owned())) {
                NumberField(
                    label = "Maximum",
                    value = Signal::derive_local(move || pedal().max),
                    min = Signal::derive_local(move || mode().range_min() + 1),
                    max = Signal::derive_local(move || mode().range_max()),
                    on_set = UnsyncCallback::new(move |v: i32| ctx.edit_pedal(|p| p.set_max(v)))
                )
                Show(when = move || pedal().message.is_fine()) {
                    text(class = "muted name") {{move || format!("fine {}", fine_pitch(pedal().max, true))}}
                }
            }
            FormRow(label = "Reversed") {
                Switch(checked = reversed, a11y:label = "Reversed")
            }
            FormRow(label = "Calibration") {
                Button(
                    variant = ButtonVariant::Outline,
                    disabled = Signal::derive_local(move || ctx.busy.get().is_some()),
                    on:click = move |_| ctx.calibrate()
                ) {"Calibrate pedal…"}
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Switches
// ------------------------------------------------------------------------------------------------

#[component]
fn SwitchPanel(ctx: Ctx, which: Switch) -> impl IntoView {
    let switch = move || ctx.config.with(|c| c.switch(which).clone());
    let mode = move || ctx.config.with(|c| c.switch(which).mode);
    let edit = move |f: &dyn Fn(&mut crate::protocol::SwitchConfig)| ctx.edit_switch(which, f);

    let message = Binding::controlled(
        Signal::derive_local(move || switch().message.key().to_owned()),
        move |key: String| {
            if let Some(message) = MessageType::from_key(&key) {
                edit(&|s| s.set_message(message));
            }
        },
    );
    let mode_binding = Binding::controlled(
        Signal::derive_local(move || mode().key()),
        move |key: String| {
            if let Some(mode) = SwitchMode::from_key(&key) {
                edit(&|s| s.set_mode(mode));
            }
        },
    );
    let linked = Binding::controlled(
        Signal::derive_local(move || ctx.config.with(|c| c.linked)),
        move |on: bool| ctx.config.update(|c| c.set_linked(on, which)),
    );
    let button_type = Binding::controlled(
        Signal::derive_local(move || {
            match switch().button_type {
                ButtonType::Latching => "latching",
                ButtonType::NonLatching => "non-latching",
            }
            .to_owned()
        }),
        move |key: String| {
            let kind = if key == "latching" { ButtonType::Latching } else { ButtonType::NonLatching };
            edit(&|s| s.button_type = kind);
        },
    );
    let button_mode = Binding::controlled(
        Signal::derive_local(move || {
            match switch().button_mode {
                ButtonMode::Momentary => "momentary",
                ButtonMode::Toggle => "toggle",
            }
            .to_owned()
        }),
        move |key: String| {
            let mode = if key == "toggle" { ButtonMode::Toggle } else { ButtonMode::Momentary };
            edit(&|s| s.button_mode = mode);
        },
    );
    let other = match which {
        Switch::One => "Button 2",
        Switch::Two => "Button 1",
    };

    view! {
        column(class = "panel") {
            Show(when = move || ctx.config.with(|c| c.linked)) {
                Alert {
                    AlertTitle {{format!("Linked with {other}")}}
                    AlertDescription {
                        "Both buttons share their settings; the step of one is the reverse of the other."
                    }
                }
            }
            FormRow(label = "MIDI message") {
                Select(value = message) {
                    SelectTrigger(label = "MIDI message", class = "wide") {SelectValue()}
                    SelectContent {
                        for choice in move || MessageType::switch_choices(ctx.info().firmware_or_latest()),
                            key = |m: &MessageType| *m
                        {
                            SelectItem(value = choice.key()) {{choice.label()}}
                        }
                    }
                }
            }
            FormRow(label = "Mode") {
                Select(value = mode_binding) {
                    SelectTrigger(label = "Mode", class = "wide") {SelectValue()}
                    SelectContent {
                        for choice in move || switch().message.switch_modes().to_vec(),
                            key = |m: &SwitchMode| *m
                        {
                            SelectItem(value = choice.key()) {{choice.label()}}
                        }
                    }
                }
            }
            FormRow(label = "") {
                text(class = "muted hint") {{move || mode().hint()}}
            }
            FormRow(label = "MIDI channel") {
                NumberField(
                    label = "MIDI channel",
                    value = Signal::derive_local(move || i32::from(switch().channel)),
                    min = 1,
                    max = 16,
                    disabled = Signal::derive_local(move || !switch().message.has_channel()),
                    on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.channel = v as u8))
                )
            }
            Show(when = move || mode().param1().is_some()) {
                FormRow(label = Signal::derive_local(move || mode().param1().map_or("", |p| p.label).to_owned())) {
                    NumberField(
                        label = "Parameter 1",
                        value = Signal::derive_local(move || switch().param1),
                        min = Signal::derive_local(move || mode().param1().map_or(0, |p| p.min)),
                        max = Signal::derive_local(move || mode().param1().map_or(127, |p| p.max)),
                        on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.set_param1(v)))
                    )
                    text(class = "muted name") {{move || {
                        let names = mode().param1().map_or(ParamNames::None, |p| p.names);
                        names.name(switch().param1).to_owned()
                    }}}
                }
            }
            Show(when = move || mode().param2().is_some()) {
                FormRow(label = Signal::derive_local(move || mode().param2().map_or("", |p| p.label).to_owned())) {
                    NumberField(
                        label = "Parameter 2",
                        value = Signal::derive_local(move || switch().param2),
                        min = Signal::derive_local(move || mode().param2().map_or(0, |p| p.min)),
                        max = Signal::derive_local(move || mode().param2().map_or(127, |p| p.max)),
                        on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.set_param2(v)))
                    )
                    text(class = "muted name") {{move || {
                        let names = mode().param2().map_or(ParamNames::None, |p| p.names);
                        names.name(switch().param2).to_owned()
                    }}}
                }
            }
            Show(when = move || mode().value1().is_some()) {
                FormRow(label = Signal::derive_local(move || mode().value1().map_or("", |v| v.label).to_owned())) {
                    NumberField(
                        label = "Value 1",
                        value = Signal::derive_local(move || switch().value1),
                        min = 0,
                        max = 127,
                        on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.set_value1(v)))
                    )
                }
            }
            Show(when = move || mode().value2().is_some()) {
                FormRow(label = Signal::derive_local(move || mode().value2().map_or("", |v| v.label).to_owned())) {
                    NumberField(
                        label = "Value 2",
                        value = Signal::derive_local(move || switch().value2),
                        min = 0,
                        max = 127,
                        on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.set_value2(v)))
                    )
                }
            }
            Show(when = move || mode().has_step()) {
                FormRow(label = "Step") {
                    NumberField(
                        label = "Step",
                        value = Signal::derive_local(move || switch().step),
                        min = Signal::derive_local(move || -mode().step_limit()),
                        max = Signal::derive_local(move || mode().step_limit()),
                        wrap = true,
                        on_set = UnsyncCallback::new(move |v: i32| edit(&|s| s.set_step(v)))
                    )
                    text(class = "muted name") {{move || {
                        if switch().step < 0 { "down" } else { "up" }.to_owned()
                    }}}
                }
                FormRow(label = "Link buttons") {
                    Switch(checked = linked, a11y:label = "Link buttons")
                }
            }
            Show(when = move || ctx.info().has_button_type()) {
                FormRow(label = "Button type") {
                    RadioGroup(
                        value = button_type,
                        orientation = Orientation::Horizontal,
                        label = "Button type"
                    ) {
                        row(class = "choices") {
                            RadioGroupItem(value = "latching", label = "Latching")
                            text {"Latching"}
                            RadioGroupItem(value = "non-latching", label = "Non-latching")
                            text {"Non-latching"}
                        }
                    }
                }
                FormRow(label = "Button mode") {
                    RadioGroup(
                        value = button_mode,
                        disabled = Signal::derive_local(move || switch().button_type == ButtonType::Latching),
                        label = "Button mode"
                    ) {
                        row(class = "choices") {
                            RadioGroupItem(value = "toggle", label = "Toggle")
                            text {"Toggle"}
                            RadioGroupItem(value = "momentary", label = "Momentary")
                            text {"Momentary"}
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Widgets
// ------------------------------------------------------------------------------------------------

/// A label on the left, controls on the right.
#[component]
fn FormRow(
    #[prop(into)] label: Signal<String, LocalStorage>,
    children: Children,
) -> impl IntoView {
    view! {
        row(class = "form-row") {
            text(class = "form-row__label") {{move || label.get()}}
            row(class = "form-row__control") {{children.into_view_once()}}
        }
    }
}

/// An integer with minus/plus buttons and a text field.
#[component]
fn NumberField(
    #[prop(into)] label: String,
    #[prop(into)] value: Signal<i32, LocalStorage>,
    #[prop(into)] min: Signal<i32, LocalStorage>,
    #[prop(into)] max: Signal<i32, LocalStorage>,
    on_set: UnsyncCallback<i32>,
    #[prop(default = false)] wrap: bool,
    #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
) -> impl IntoView {
    let step = move |delta: i32| {
        let (low, high) = (min.get_untracked(), max.get_untracked());
        let mut next = value.get_untracked() + delta;
        if wrap {
            if next > high {
                next = low;
            } else if next < low {
                next = high;
            }
        }
        on_set.run(next.clamp(low, high));
    };
    let text = Binding::controlled(
        Signal::derive_local(move || value.get().to_string()),
        move |typed: String| {
            if let Ok(number) = typed.trim().parse::<i32>() {
                on_set.run(number.clamp(min.get_untracked(), max.get_untracked()));
            }
        },
    );
    let less = format!("Decrease {label}");
    let more = format!("Increase {label}");
    view! {
        row(class = "number") {
            Button(
                variant = ButtonVariant::Outline,
                size = ButtonSize::IconSm,
                disabled = Signal::derive_local(move || {
                    disabled.get() || (!wrap && value.get() <= min.get())
                }),
                a11y:label = less,
                on:click = move |_| step(-1)
            ) {"−"}
            Input(value = text, label = label, disabled = disabled, class = "number__input")
            Button(
                variant = ButtonVariant::Outline,
                size = ButtonSize::IconSm,
                disabled = Signal::derive_local(move || {
                    disabled.get() || (!wrap && value.get() >= max.get())
                }),
                a11y:label = more,
                on:click = move |_| step(1)
            ) {"+"}
        }
    }
}

pub const SHEET: &str = zgui::css!(
    ":root {
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-family: sans-serif;
        font-size: var(--zui-type-size-md);
        overflow: auto;
    }

    .app {
        min-height: 100%;
        padding: var(--zui-space-xl);
        gap: var(--zui-space-lg);
    }

    .header { align-items: center; gap: var(--zui-space-md); }
    .header__text { gap: var(--zui-space-xs); }
    .title { font-size: var(--zui-type-size-2xl); font-weight: 700; }
    .muted { color: var(--zui-color-muted-foreground); font-size: var(--zui-type-size-sm); }

    .tabs { flex-grow: 1; gap: var(--zui-space-md); }
    .panel {
        gap: var(--zui-space-md);
        padding: var(--zui-space-lg);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-lg);
        background-color: var(--zui-color-card);
        color: var(--zui-color-card-foreground);
    }

    .form-row { align-items: center; gap: var(--zui-space-md); min-height: 36px; }
    .form-row__label {
        width: 150px;
        flex-shrink: 0;
        font-size: var(--zui-type-size-sm);
        color: var(--zui-color-muted-foreground);
    }
    .form-row__control { align-items: center; gap: var(--zui-space-md); flex-grow: 1; }
    .wide { min-width: 280px; }
    .name { min-width: 120px; }
    .hint { max-width: 380px; margin-top: -6px; }
    .choices { align-items: center; gap: var(--zui-space-sm); }

    .number { align-items: center; gap: var(--zui-space-xs); }
    .number__input { width: 64px; }

    .empty {
        flex-grow: 1;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-md);
        border: 1px dashed var(--zui-color-border);
        border-radius: var(--zui-radius-lg);
        padding: var(--zui-space-xl);
    }
    .empty__title { font-size: var(--zui-type-size-lg); font-weight: 600; }
    .empty__text { max-width: 360px; text-align: center; }

    .footer { align-items: center; gap: var(--zui-space-sm); }

    .busy {
        position: absolute;
        left: 0; top: 0; right: 0; bottom: 0;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-md);
        background-color: color-mix(in srgb, var(--zui-color-background) 80%, transparent);
    }
    .busy__label { font-weight: 600; }
    .busy__calibration { gap: var(--zui-space-sm); width: 320px; align-items: center; }"
);
