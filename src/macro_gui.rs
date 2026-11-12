use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
};

use mlua::{Error, Function, Lua, RegistryKey, UserData, UserDataFields, UserDataMethods, Value};

use crate::{
    runtime::{integer, strict_text, Output},
    shortcut::{self, Combination, Key},
};

pub mod immediate;
#[cfg(target_os = "linux")]
pub mod tk;
#[cfg(windows)]
pub mod windows;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Basic,
    Imgui,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Label,
    Button,
    Slider { min: i32, max: i32 },
    Toggle,
    Input,
    Keybind,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ControlValue {
    None,
    Text(String),
    Number(i32),
    Boolean(bool),
}

impl ControlValue {
    fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::None => Value::Nil,
            Self::Text(text) => Value::String(lua.create_string(text)?),
            Self::Number(value) => Value::Integer((*value).into()),
            Self::Boolean(value) => Value::Boolean(*value),
        })
    }
}

pub struct Control {
    pub label: String,
    pub kind: Kind,
    pub value: ControlValue,
    pub hovered: bool,
}

impl Control {
    pub fn keybind_caption(&self, recording: bool) -> String {
        let value = match (&self.value, recording) {
            (_, true) => "Press a key combination...",
            (ControlValue::Text(combo), false) => combo,
            _ => "Click to record",
        };
        format!("{}: {value}", self.label)
    }
}

pub struct Window {
    pub title: String,
    pub visible: bool,
    pub closed: bool,
    pub controls: Vec<Control>,
    pub recording: Option<usize>,
    pub appearance: Appearance,
    engine: Engine,
}

#[derive(Default)]
pub struct Appearance {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub theme: Theme,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub header: [f32; 4],
    pub background: [f32; 4],
    pub control: [f32; 4],
    pub accent: [f32; 4],
    pub text: [f32; 4],
    pub rounding: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            header: [0.13, 0.18, 0.26, 1.0],
            background: [0.08, 0.11, 0.16, 1.0],
            control: [0.17, 0.23, 0.32, 1.0],
            accent: [0.43, 0.68, 1.0, 1.0],
            text: [0.93, 0.96, 1.0, 1.0],
            rounding: 12.0,
        }
    }
}

fn window_options(value: Value, engine: Engine) -> mlua::Result<(bool, Appearance)> {
    let Value::Table(options) = value else {
        return Ok((optional_bool(value, true)?, Appearance::default()));
    };
    if engine != Engine::Imgui {
        return Err(Error::runtime(
            "Window size and theme options require the Imgui engine.",
        ));
    }
    let dimension = |name, min| {
        let value: Value = options.raw_get(name)?;
        if value.is_nil() {
            return Ok(None);
        }
        let value = integer(value)?;
        if !(min..=4096).contains(&value) {
            return Err(Error::runtime(format!(
                "Window {name} must be between {min} and 4096."
            )));
        }
        Ok(Some(value as u32))
    };
    let mut theme = Theme::default();
    match options.raw_get::<Value>("theme")? {
        Value::Nil => {}
        Value::Table(colors) => {
            let color = |name, default| -> mlua::Result<[f32; 4]> {
                let value: Value = colors.raw_get(name)?;
                if value.is_nil() {
                    return Ok(default);
                }
                let value = strict_text(value)?;
                let value = value.to_str()?;
                let digits = value.strip_prefix('#').filter(|hex| {
                    hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
                });
                let rgb = digits
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .ok_or_else(|| {
                        Error::runtime(format!("Theme {name} must be a color like #20304A."))
                    })?;
                Ok([
                    ((rgb >> 16) & 0xff) as f32 / 255.0,
                    ((rgb >> 8) & 0xff) as f32 / 255.0,
                    (rgb & 0xff) as f32 / 255.0,
                    1.0,
                ])
            };
            theme.header = color("header", theme.header)?;
            theme.background = color("background", theme.background)?;
            theme.control = color("control", theme.control)?;
            theme.accent = color("accent", theme.accent)?;
            theme.text = color("text", theme.text)?;
            let rounding: Value = colors.raw_get("rounding")?;
            if !rounding.is_nil() {
                let rounding = integer(rounding)?;
                if !(0..=32).contains(&rounding) {
                    return Err(Error::runtime("Theme rounding must be between 0 and 32."));
                }
                theme.rounding = rounding as f32;
            }
        }
        _ => return Err(Error::runtime("Theme options must be a table.")),
    }
    Ok((
        optional_bool(options.raw_get("visible")?, true)?,
        Appearance {
            width: dimension("width", 240)?,
            height: dimension("height", 130)?,
            theme,
        },
    ))
}

pub enum Event {
    Press(usize),
    Hover(usize, bool),
    Change(usize, ControlValue),
    Record(usize),
    CancelRecording(usize),
    Key(usize, Combination),
    Close,
}

pub enum Update {
    Visibility(bool),
    Control(usize),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Signal {
    Pressed(usize),
    Hovered(usize),
    Unhovered(usize),
    Changed(usize),
    Closed,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Self::Pressed(_) => "pressed",
            Self::Hovered(_) => "hovered",
            Self::Unhovered(_) => "unhovered",
            Self::Changed(_) => "changed",
            Self::Closed => "closed",
        }
    }
}

#[derive(Default)]
struct State {
    window: Option<Window>,
    running: bool,
    callbacks: HashMap<Signal, Vec<RegistryKey>>,
    signals: VecDeque<(Signal, ControlValue)>,
    updates: VecDeque<Update>,
}

impl State {
    fn window_mut(&mut self) -> &mut Window {
        self.window.as_mut().expect("window handle")
    }

    fn change(&mut self, id: usize, value: ControlValue, update: bool) {
        let window = self.window_mut();
        let control = &mut window.controls[id];
        if window.closed || control.value == value {
            return;
        }
        control.value = value.clone();
        if control.kind != Kind::Label {
            self.signals.push_back((Signal::Changed(id), value));
        }
        if update {
            self.updates.push_back(Update::Control(id));
        }
    }

    fn hover(&mut self, id: usize, hovered: bool) {
        let control = &mut self.window_mut().controls[id];
        if control.hovered != hovered {
            control.hovered = hovered;
            let signal = if hovered {
                Signal::Hovered(id)
            } else {
                Signal::Unhovered(id)
            };
            self.signals.push_back((signal, ControlValue::None));
        }
    }

    fn clear_hover(&mut self) {
        let controls = &mut self.window.as_mut().expect("window handle").controls;
        for (id, control) in controls.iter_mut().enumerate() {
            if std::mem::take(&mut control.hovered) {
                self.signals
                    .push_back((Signal::Unhovered(id), ControlValue::None));
            }
        }
    }

    fn close(&mut self) {
        let window = self.window_mut();
        if window.closed {
            return;
        }
        window.closed = true;
        window.visible = false;
        self.cancel_recording();
        self.clear_hover();
        self.signals.push_back((Signal::Closed, ControlValue::None));
        self.updates.push_back(Update::Close);
    }

    fn cancel_recording(&mut self) {
        if let Some(id) = self.window_mut().recording.take() {
            self.updates.push_back(Update::Control(id));
        }
    }

    fn record_key(&mut self, combo: Combination) -> bool {
        let Some(id) = self.window.as_ref().and_then(|window| window.recording) else {
            return false;
        };
        self.cancel_recording();
        if combo.key != Key::Escape || combo.modifiers != 0 {
            self.change(id, ControlValue::Text(combo.to_string()), false);
        }
        true
    }
}

#[derive(Default)]
pub struct Gui(Rc<RefCell<State>>);

impl Gui {
    pub fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let state = Rc::downgrade(&self.0);
        lua.globals()
            .set("gui", EngineHandle(state.clone(), Engine::Basic))?;
        lua.globals().set(
            "defineGui",
            lua.create_function(move |_, name: Value| {
                let name = strict_text(name)?.to_str()?.to_owned();
                let engine = if name.eq_ignore_ascii_case("basic") {
                    Engine::Basic
                } else if name.eq_ignore_ascii_case("imgui") {
                    Engine::Imgui
                } else {
                    return Err(Error::runtime("Choose the Basic or Imgui GUI engine."));
                };
                Ok(EngineHandle(state.clone(), engine))
            })?,
        )
    }

    pub fn start(&self) {
        self.0.borrow_mut().running = true;
    }

    pub fn window(&self) -> Ref<'_, Option<Window>> {
        Ref::map(self.0.borrow(), |state| &state.window)
    }

    pub fn engine(&self) -> Option<Engine> {
        self.window()
            .as_ref()
            .filter(|window| !window.closed)
            .map(|window| window.engine)
    }

    pub fn updates(&self) -> VecDeque<Update> {
        std::mem::take(&mut self.0.borrow_mut().updates)
    }

    pub fn recording(&self) -> Option<usize> {
        self.window().as_ref().and_then(|window| window.recording)
    }

    pub fn record_key(&self, combo: Combination) -> bool {
        self.0.borrow_mut().record_key(combo)
    }

    pub fn event(&self, event: Event) {
        let mut state = self.0.borrow_mut();
        let Some(window) = state.window.as_ref().filter(|window| !window.closed) else {
            return;
        };
        let kind = |id| {
            window
                .controls
                .get(id)
                .map(|control: &Control| control.kind)
        };
        match event {
            Event::Press(id) if kind(id) == Some(Kind::Button) => state
                .signals
                .push_back((Signal::Pressed(id), ControlValue::None)),
            Event::Hover(id, hovered) if window.visible && kind(id) == Some(Kind::Button) => {
                state.hover(id, hovered);
            }
            Event::Change(id, ControlValue::Number(value)) => {
                if let Some(Kind::Slider { min, max }) = kind(id) {
                    if (min..=max).contains(&value) {
                        state.change(id, ControlValue::Number(value), false);
                    }
                }
            }
            Event::Change(id, ControlValue::Boolean(value)) if kind(id) == Some(Kind::Toggle) => {
                state.change(id, ControlValue::Boolean(value), false);
            }
            Event::Change(id, ControlValue::Text(value)) if kind(id) == Some(Kind::Input) => {
                let normalized = value.contains(['\0', '\r', '\n']);
                let value = if normalized {
                    value.replace(['\0', '\r', '\n'], " ")
                } else {
                    value
                };
                state.change(id, ControlValue::Text(value), false);
                if normalized {
                    state.updates.push_back(Update::Control(id));
                }
            }
            Event::Record(id) if window.visible && kind(id) == Some(Kind::Keybind) => {
                let already_recording = window.recording == Some(id);
                state.cancel_recording();
                if !already_recording {
                    state.window_mut().recording = Some(id);
                    state.updates.push_back(Update::Control(id));
                }
            }
            Event::CancelRecording(id) if window.recording == Some(id) => {
                state.cancel_recording();
            }
            Event::Key(id, combo) if window.recording == Some(id) => {
                state.record_key(combo);
            }
            Event::Close => state.close(),
            _ => {}
        }
    }

    pub fn dispatch(&self, lua: &Lua, output: Output) {
        loop {
            let Some((signal, value)) = self.0.borrow_mut().signals.pop_front() else {
                break;
            };
            let callbacks = self
                .0
                .borrow()
                .callbacks
                .get(&signal)
                .into_iter()
                .flatten()
                .map(|key| lua.registry_value::<Function>(key))
                .collect::<Vec<_>>();
            for callback in callbacks {
                let result = callback.and_then(|callback| match signal {
                    Signal::Changed(_) => callback.call::<()>(value.to_lua(lua)?),
                    _ => callback.call::<()>(()),
                });
                if let Err(error) = result {
                    output('E', &format!("GUI {} callback — {error}", signal.name()));
                }
            }
        }
    }
}

type StateHandle = Weak<RefCell<State>>;

fn state(handle: &StateHandle) -> mlua::Result<Rc<RefCell<State>>> {
    handle
        .upgrade()
        .ok_or_else(|| Error::runtime("The GUI has stopped."))
}

fn text(value: Value) -> mlua::Result<String> {
    let value = strict_text(value)?.to_str()?.to_owned();
    if value.contains(['\0', '\r', '\n']) {
        return Err(Error::runtime("Expected a single line of GUI text."));
    }
    Ok(value)
}

fn boolean(value: Value) -> mlua::Result<bool> {
    match value {
        Value::Boolean(value) => Ok(value),
        _ => Err(Error::runtime("Expected a boolean.")),
    }
}

fn optional_bool(value: Value, default: bool) -> mlua::Result<bool> {
    if value.is_nil() {
        Ok(default)
    } else {
        boolean(value)
    }
}

struct EngineHandle(StateHandle, Engine);
struct WindowHandle(StateHandle);
struct ControlHandle(StateHandle, usize);
struct SignalHandle(StateHandle, Signal);

impl UserData for EngineHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("window", |_, this, (title, options): (Value, Value)| {
            let title = text(title)?;
            let (visible, appearance) = window_options(options, this.1)?;
            let state = state(&this.0)?;
            let mut state = state.borrow_mut();
            if state.running || state.window.is_some() {
                return Err(Error::runtime(
                    "Declare one GUI window when the script starts.",
                ));
            }
            state.window = Some(Window {
                title,
                visible,
                closed: false,
                controls: Vec::new(),
                recording: None,
                appearance,
                engine: this.1,
            });
            Ok(WindowHandle(this.0.clone()))
        });
    }
}

impl WindowHandle {
    fn add(&self, label: String, kind: Kind, value: ControlValue) -> mlua::Result<ControlHandle> {
        let state = state(&self.0)?;
        let mut state = state.borrow_mut();
        if state.running || state.window_mut().closed {
            return Err(Error::runtime(
                "Declare GUI controls before the script starts listening.",
            ));
        }
        let controls = &mut state.window_mut().controls;
        let id = controls.len();
        controls.push(Control {
            label,
            kind,
            value,
            hovered: false,
        });
        Ok(ControlHandle(self.0.clone(), id))
    }

    fn visibility(&self, visible: bool) -> mlua::Result<()> {
        let state = state(&self.0)?;
        let mut state = state.borrow_mut();
        let window = state.window_mut();
        if window.closed {
            return Err(Error::runtime("This GUI window is closed."));
        }
        if window.visible != visible {
            window.visible = visible;
            if !visible {
                state.clear_hover();
                state.cancel_recording();
            }
            state.updates.push_back(Update::Visibility(visible));
        }
        Ok(())
    }
}

impl UserData for WindowHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("closed", |_, this| {
            Ok(SignalHandle(this.0.clone(), Signal::Closed))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("label", |_, this, value: Value| {
            this.add(String::new(), Kind::Label, ControlValue::Text(text(value)?))
        });
        methods.add_method("button", |_, this, label: Value| {
            this.add(text(label)?, Kind::Button, ControlValue::None)
        });
        methods.add_method(
            "slider",
            |_, this, (label, min, max, initial): (Value, Value, Value, Value)| {
                let min = integer(min)?;
                let max = integer(max)?;
                if min >= max || max.checked_sub(min).is_none() {
                    return Err(Error::runtime(
                        "A slider requires min < max and a 32-bit range.",
                    ));
                }
                let value = if initial.is_nil() {
                    min
                } else {
                    slider_value(initial, min, max)?
                };
                this.add(
                    text(label)?,
                    Kind::Slider { min, max },
                    ControlValue::Number(value),
                )
            },
        );
        methods.add_method("toggle", |_, this, (label, initial): (Value, Value)| {
            this.add(
                text(label)?,
                Kind::Toggle,
                ControlValue::Boolean(optional_bool(initial, false)?),
            )
        });
        methods.add_method("input", |_, this, (label, initial): (Value, Value)| {
            let initial = if initial.is_nil() {
                String::new()
            } else {
                text(initial)?
            };
            this.add(text(label)?, Kind::Input, ControlValue::Text(initial))
        });
        methods.add_method("keybind", |_, this, (label, initial): (Value, Value)| {
            this.add(text(label)?, Kind::Keybind, keybind_value(initial)?)
        });
        methods.add_method("show", |_, this, ()| this.visibility(true));
        methods.add_method("hide", |_, this, ()| this.visibility(false));
        methods.add_method("close", |_, this, ()| {
            state(&this.0)?.borrow_mut().close();
            Ok(())
        });
    }
}

fn slider_value(value: Value, min: i32, max: i32) -> mlua::Result<i32> {
    let value = integer(value)?;
    if !(min..=max).contains(&value) {
        return Err(Error::runtime(format!(
            "Slider value must be between {min} and {max}."
        )));
    }
    Ok(value)
}

fn keybind_value(value: Value) -> mlua::Result<ControlValue> {
    if value.is_nil() {
        return Ok(ControlValue::None);
    }
    let value = strict_text(value)?;
    let combo = shortcut::parse(&value.to_str()?).map_err(Error::runtime)?;
    Ok(ControlValue::Text(combo.to_string()))
}

impl ControlHandle {
    fn signal(&self, signal: Signal) -> mlua::Result<SignalHandle> {
        let state = state(&self.0)?;
        let state = state.borrow();
        let kind = state.window.as_ref().expect("control handle").controls[self.1].kind;
        let available = match signal {
            Signal::Changed(_) => matches!(
                kind,
                Kind::Slider { .. } | Kind::Toggle | Kind::Input | Kind::Keybind
            ),
            _ => kind == Kind::Button,
        };
        if !available {
            return Err(Error::runtime("This control does not provide that signal."));
        }
        Ok(SignalHandle(self.0.clone(), signal))
    }
}

impl UserData for ControlHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("pressed", |_, this| this.signal(Signal::Pressed(this.1)));
        fields.add_field_method_get("hovered", |_, this| this.signal(Signal::Hovered(this.1)));
        fields.add_field_method_get("unhovered", |_, this| {
            this.signal(Signal::Unhovered(this.1))
        });
        fields.add_field_method_get("changed", |_, this| this.signal(Signal::Changed(this.1)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, ()| {
            let state = state(&this.0)?;
            let state = state.borrow();
            state.window.as_ref().expect("control handle").controls[this.1]
                .value
                .to_lua(lua)
        });
        methods.add_method("set", |_, this, value: Value| {
            let state = state(&this.0)?;
            let mut state = state.borrow_mut();
            let window = state.window_mut();
            if window.closed {
                return Err(Error::runtime("This GUI window is closed."));
            }
            let value = match window.controls[this.1].kind {
                Kind::Button => return Err(Error::runtime("A button has no editable value.")),
                Kind::Label | Kind::Input => ControlValue::Text(text(value)?),
                Kind::Toggle => ControlValue::Boolean(boolean(value)?),
                Kind::Keybind => keybind_value(value)?,
                Kind::Slider { min, max } => ControlValue::Number(slider_value(value, min, max)?),
            };
            state.change(this.1, value, true);
            Ok(())
        });
    }
}

impl UserData for SignalHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("connect", |lua, this, callback: Function| {
            let key = lua.create_registry_value(callback)?;
            state(&this.0)?
                .borrow_mut()
                .callbacks
                .entry(this.1)
                .or_default()
                .push(key);
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imgui_window_options_preserve_fixed_size_and_theme() {
        let lua = Lua::new();
        let gui = Gui::default();
        gui.install(&lua).unwrap();
        lua.load(
            r##"
            local engine = defineGui("Imgui")
            assert(not pcall(function()
                engine:window("Bad theme", {theme = {header = "blue"}})
            end))
            engine:window("Appearance", {
                width = 480, height = 320, visible = false,
                theme = {header = "#FFFFFF", background = "#000000", rounding = 16},
            })
        "##,
        )
        .exec()
        .unwrap();
        let window = gui.window();
        let window = window.as_ref().unwrap();
        assert!(!window.visible);
        assert_eq!(window.appearance.width, Some(480));
        assert_eq!(window.appearance.height, Some(320));
        assert_eq!(window.appearance.theme.header, [1.0; 4]);
        assert_eq!(window.appearance.theme.background, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(window.appearance.theme.rounding, 16.0);
    }

    #[test]
    fn gui_signals_deliver_values_once_and_allow_callbacks_to_update_controls() {
        let lua = Lua::new();
        let gui = Gui::default();
        gui.install(&lua).unwrap();
        lua.load(
            r#"
            local window = defineGui("Imgui"):window("Settings")
            local button = window:button("Apply")
            local slider = window:slider("Delay", -2, 10, 0)
            local toggle = window:toggle("Enabled", false)
            local input = window:input("Message", "Hello")
            local label = window:label("Ready")
            trace = {}
            local function push(value) table.insert(trace, tostring(value)) end
            button.hovered:connect(function() push("hovered") end)
            button.unhovered:connect(function() push("unhovered") end)
            button.pressed:connect(function()
                slider:set(3)
                slider:set(3)
                toggle:set(true)
                label:set(input:get())
            end)
            button.pressed:connect(function() push(label:get()) end)
            slider.changed:connect(push)
            toggle.changed:connect(push)
            input.changed:connect(push)
            window.closed:connect(function() push("closed") end)
            assert(not pcall(function() slider:set(1.5) end))
            assert(not pcall(function() slider:set(11) end))
        "#,
        )
        .exec()
        .unwrap();
        gui.start();
        let output = |kind, message: &str| assert_ne!(kind, 'E', "{message}");
        for event in [
            Event::Hover(0, true),
            Event::Hover(0, true),
            Event::Change(1, ControlValue::Number(-1)),
            Event::Change(1, ControlValue::Number(1)),
            Event::Change(3, ControlValue::Text("Updated".into())),
            Event::Press(0),
            Event::Close,
            Event::Close,
        ] {
            gui.event(event);
            gui.dispatch(&lua, output);
        }
        let trace: String = lua.load("return table.concat(trace, ',')").eval().unwrap();
        assert_eq!(
            trace,
            "hovered,-1,1,Updated,Updated,3,true,unhovered,closed"
        );
        assert!(gui.engine().is_none());
    }

    #[test]
    fn keybind_recording_cancels_and_emits_normalized_combos() {
        let lua = Lua::new();
        let gui = Gui::default();
        gui.install(&lua).unwrap();
        lua.load(
            r#"
            local window = gui:window("Keys")
            outgoing = window:keybind("Outgoing", "control+s")
            assert(outgoing:get() == "Ctrl+S")
            trace = {}
            outgoing.changed:connect(function(value)
                table.insert(trace, tostring(value))
            end)
        "#,
        )
        .exec()
        .unwrap();
        gui.start();
        let output = |kind, message: &str| assert_ne!(kind, 'E', "{message}");
        let escape = shortcut::parse("Escape").unwrap();
        let combo = shortcut::parse("shift+ctrl+1").unwrap();
        gui.event(Event::Record(0));
        gui.event(Event::Key(0, escape));
        assert!(gui.recording().is_none());
        gui.event(Event::Record(0));
        assert!(gui.record_key(combo));
        assert!(!gui.record_key(combo));
        gui.dispatch(&lua, output);
        gui.event(Event::Record(0));
        gui.event(Event::Key(0, combo));
        lua.load("assert(outgoing:get() == 'Ctrl+Shift+1'); outgoing:set(nil)")
            .exec()
            .unwrap();
        gui.dispatch(&lua, output);
        let trace: String = lua.load("return table.concat(trace, ',')").eval().unwrap();
        assert_eq!(trace, "Ctrl+Shift+1,nil");
    }
}
