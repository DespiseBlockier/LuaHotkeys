use super::*;

#[derive(Clone)]
pub(super) struct Panel(pub StateHandle, pub Option<usize>);

pub(super) fn require_imgui(window: &Window) -> mlua::Result<()> {
    if window.closed {
        Err(Error::runtime("This GUI window is closed."))
    } else if window.engine != Engine::Imgui {
        Err(Error::runtime("This feature requires the Imgui engine."))
    } else {
        Ok(())
    }
}

impl Panel {
    fn add(
        &self,
        label: String,
        kind: Kind,
        value: ControlValue,
        options: Vec<String>,
    ) -> mlua::Result<ControlHandle> {
        let state = state(&self.0)?;
        let mut state = state.borrow_mut();
        let running = state.running;
        let window = state.window_mut();
        if window.closed || running && window.engine == Engine::Basic {
            return Err(Error::runtime(
                "Declare Basic GUI controls before listening.",
            ));
        }
        if kind.is_group() || matches!(kind, Kind::Dropdown | Kind::Color) {
            require_imgui(window)?;
        }
        if let Some(parent) = self.1 {
            let control = ControlHandle(self.0.clone(), parent);
            let group = control.control(window)?;
            if !group.kind.is_group() {
                return Err(Error::runtime(
                    "Add controls to a window, tab, folder, or dock.",
                ));
            }
            let mut ancestor = group.parent;
            for _ in 0..32 {
                let Some(id) = ancestor else { break };
                ancestor = window.controls[id].parent;
            }
            if ancestor.is_some() {
                return Err(Error::runtime("GUI groups can be nested up to 32 levels."));
            }
        }
        let id = window.controls.len();
        window.controls.push(Control {
            label,
            kind,
            value,
            hovered: false,
            parent: self.1,
            children: Vec::new(),
            options,
            tint: None,
            removed: false,
        });
        if let Some(parent) = self.1 {
            window.controls[parent].children.push(id);
        }
        if kind == Kind::Tab && window.active_tab.is_none() {
            window.active_tab = Some(id);
        }
        Ok(ControlHandle(self.0.clone(), id))
    }

    pub(super) fn group(
        &self,
        label: String,
        kind: Kind,
        value: ControlValue,
    ) -> mlua::Result<ControlHandle> {
        self.add(label, kind, value, Vec::new())
    }
}

pub(super) fn selection(options: &[String], value: Value) -> mlua::Result<ControlValue> {
    if value.is_nil() {
        return Ok(ControlValue::None);
    }
    let value = text(value)?;
    if !options.contains(&value) {
        return Err(Error::runtime("Choose an existing dropdown option."));
    }
    Ok(ControlValue::Text(value))
}

fn option(value: Value) -> mlua::Result<String> {
    let value = text(value)?;
    if value.trim().is_empty() {
        return Err(Error::runtime("Dropdown options must have a name."));
    }
    Ok(value)
}

pub(super) fn add_methods<T: UserData + 'static, M: UserDataMethods<T>>(
    methods: &mut M,
    panel: fn(&T) -> Panel,
) {
    methods.add_method("label", move |_, this, value: Value| {
        panel(this).group(String::new(), Kind::Label, ControlValue::Text(text(value)?))
    });
    methods.add_method("button", move |_, this, label: Value| {
        panel(this).group(text(label)?, Kind::Button, ControlValue::None)
    });
    methods.add_method(
        "slider",
        move |_, this, (label, min, max, initial): (Value, Value, Value, Value)| {
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
            panel(this).group(
                text(label)?,
                Kind::Slider { min, max },
                ControlValue::Number(value),
            )
        },
    );
    methods.add_method(
        "toggle",
        move |_, this, (label, initial): (Value, Value)| {
            panel(this).group(
                text(label)?,
                Kind::Toggle,
                ControlValue::Boolean(optional_bool(initial, false)?),
            )
        },
    );
    methods.add_method("input", move |_, this, (label, initial): (Value, Value)| {
        let initial = if initial.is_nil() {
            String::new()
        } else {
            text(initial)?
        };
        panel(this).group(text(label)?, Kind::Input, ControlValue::Text(initial))
    });
    methods.add_method(
        "keybind",
        move |_, this, (label, initial): (Value, Value)| {
            panel(this).group(text(label)?, Kind::Keybind, keybind_value(initial)?)
        },
    );
    methods.add_method("folder", move |_, this, (label, open): (Value, Value)| {
        panel(this).group(
            text(label)?,
            Kind::Folder,
            ControlValue::Boolean(optional_bool(open, false)?),
        )
    });
    methods.add_method("dock", move |_, this, ()| {
        panel(this).group(String::new(), Kind::Dock, ControlValue::None)
    });
    methods.add_method("color", move |_, this, (label, initial): (Value, Value)| {
        let initial = if initial.is_nil() {
            0xFF85A2
        } else {
            rgb(initial)?
        };
        panel(this).group(text(label)?, Kind::Color, ControlValue::Color(initial))
    });
    methods.add_method(
        "dropdown",
        move |_, this, (label, options, initial): (Value, Table, Value)| {
            let mut names = Vec::new();
            for value in options.sequence_values::<Value>() {
                let name = option(value?)?;
                if names.contains(&name) {
                    return Err(Error::runtime("Dropdown option names must be unique."));
                }
                names.push(name);
            }
            let selected = selection(&names, initial)?;
            panel(this).add(text(label)?, Kind::Dropdown, selected, names)
        },
    );
}

pub(super) fn control_methods<M: UserDataMethods<ControlHandle>>(methods: &mut M) {
    add_methods(methods, |this| Panel(this.0.clone(), Some(this.1)));
    methods.add_method("select", |_, this, ()| {
        let state = state(&this.0)?;
        let mut state = state.borrow_mut();
        let window = state.window_mut();
        require_imgui(window)?;
        if this.control(window)?.kind != Kind::Tab {
            return Err(Error::runtime("Only a tab can be selected."));
        }
        state.select_tab(this.1);
        Ok(())
    });
    methods.add_method("switch", |_, this, ()| {
        let state = state(&this.0)?;
        let mut state = state.borrow_mut();
        let window = state.window_mut();
        if window.closed {
            return Err(Error::runtime("This GUI window is closed."));
        }
        let value = match this.control(window)? {
            Control {
                kind: Kind::Toggle | Kind::Folder,
                value: ControlValue::Boolean(value),
                ..
            } => !value,
            _ => return Err(Error::runtime("Only a toggle or folder can be switched.")),
        };
        state.change(this.1, ControlValue::Boolean(value), true);
        Ok(())
    });
    methods.add_method("set_color", |_, this, value: Value| {
        let value = if value.is_nil() {
            None
        } else {
            Some(rgb(value)?)
        };
        let state = state(&this.0)?;
        let mut state = state.borrow_mut();
        let window = state.window_mut();
        require_imgui(window)?;
        this.control(window)?;
        window.controls[this.1].tint = value;
        Ok(())
    });
    methods.add_method("get_color", |_, this, ()| {
        let state = state(&this.0)?;
        let state = state.borrow();
        let window = state.window.as_ref().expect("control handle");
        require_imgui(window)?;
        Ok(this.control(window)?.tint.map(|rgb| format!("#{rgb:06X}")))
    });
    for (method, add) in [("add_option", true), ("remove_option", false)] {
        methods.add_method(method, move |_, this, value: Value| {
            let name = option(value)?;
            let state = state(&this.0)?;
            let mut state = state.borrow_mut();
            let window = state.window_mut();
            require_imgui(window)?;
            if this.control(window)?.kind != Kind::Dropdown {
                return Err(Error::runtime("Only a dropdown has options."));
            }
            let control = &mut window.controls[this.1];
            if add {
                if control.options.contains(&name) {
                    return Err(Error::runtime("Dropdown option names must be unique."));
                }
                control.options.push(name);
            } else {
                control.options.retain(|option| option != &name);
                if control.value == ControlValue::Text(name) {
                    state.change(this.1, ControlValue::None, true);
                }
            }
            Ok(())
        });
    }
    methods.add_method("destroy", |_, this, ()| {
        let state = state(&this.0)?;
        let mut state = state.borrow_mut();
        let window = state.window_mut();
        require_imgui(window)?;
        if window.controls[this.1].removed {
            return Ok(());
        }
        if let Some(parent) = window.controls[this.1].parent {
            window.controls[parent]
                .children
                .retain(|&child| child != this.1);
        }
        let mut pending = vec![this.1];
        while let Some(id) = pending.pop() {
            let control = &mut state.window_mut().controls[id];
            control.removed = true;
            control.value = ControlValue::None;
            control.label = String::new();
            control.options = Vec::new();
            pending.append(&mut control.children);
            for signal in [
                Signal::Pressed(id),
                Signal::Hovered(id),
                Signal::Unhovered(id),
                Signal::Changed(id),
            ] {
                state.callbacks.remove(&signal);
            }
        }
        let window = state.window_mut();
        if window
            .active_tab
            .is_some_and(|id| window.controls[id].removed)
        {
            window.active_tab = window
                .controls
                .iter()
                .position(|control| !control.removed && control.kind == Kind::Tab);
        }
        if window
            .recording
            .is_some_and(|id| window.controls[id].removed)
        {
            state.cancel_recording();
        }
        Ok(())
    });
}
