use super::{
    value, Item, CONNECTION_METHODS, CONTROL_METHODS, ENGINE_METHODS, SIGNAL_METHODS,
    WINDOW_METHODS,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Engine {
    Basic,
    Imgui,
    Unspecified,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Engine,
    Window,
    Tab,
    Folder,
    Dock,
    Label,
    Button,
    Slider,
    Toggle,
    Input,
    Keybind,
    Dropdown,
    Color,
    Signal,
    Connection,
}

#[derive(Clone, Copy)]
pub(super) struct GuiType {
    kind: Kind,
    engine: Engine,
}

const SIGNAL_FIELDS: &[Item] = &[
    value("closed", "Window closed signal"),
    value("pressed", "Button pressed signal"),
    value("hovered", "Button hovered signal"),
    value("unhovered", "Button unhovered signal"),
    value("changed", "Value changed signal"),
];

impl GuiType {
    pub(super) fn engine(engine: Engine) -> Self {
        Self {
            kind: Kind::Engine,
            engine,
        }
    }

    pub(super) fn detail(self) -> &'static str {
        match self.kind {
            Kind::Engine => "GUI engine",
            Kind::Window => "GUI window",
            Kind::Tab => "GUI tab",
            Kind::Folder => "GUI folder",
            Kind::Dock => "GUI dock",
            Kind::Label => "GUI label",
            Kind::Button => "GUI button",
            Kind::Slider => "GUI slider",
            Kind::Toggle => "GUI toggle",
            Kind::Input => "GUI input",
            Kind::Keybind => "GUI keybind picker",
            Kind::Dropdown => "GUI dropdown",
            Kind::Color => "GUI color picker",
            Kind::Signal => "GUI signal",
            Kind::Connection => "GUI connection",
        }
    }

    fn with_kind(self, kind: Kind) -> Self {
        Self { kind, ..self }
    }

    fn panel(self) -> bool {
        matches!(
            self.kind,
            Kind::Window | Kind::Tab | Kind::Folder | Kind::Dock
        )
    }

    fn control(self) -> bool {
        !matches!(
            self.kind,
            Kind::Engine | Kind::Window | Kind::Signal | Kind::Connection
        )
    }

    fn has_value(self) -> bool {
        matches!(
            self.kind,
            Kind::Label
                | Kind::Slider
                | Kind::Toggle
                | Kind::Input
                | Kind::Keybind
                | Kind::Dropdown
                | Kind::Color
                | Kind::Folder
        )
    }

    pub(super) fn result(self, method: &str) -> Option<Self> {
        let kind = match method {
            "window" if self.kind == Kind::Engine => Kind::Window,
            "connect" if self.kind == Kind::Signal => Kind::Connection,
            "tab" if self.kind == Kind::Window && self.engine != Engine::Basic => Kind::Tab,
            name if self.panel() => match name {
                "label" => Kind::Label,
                "button" => Kind::Button,
                "slider" => Kind::Slider,
                "toggle" => Kind::Toggle,
                "input" => Kind::Input,
                "keybind" => Kind::Keybind,
                "folder" if self.engine != Engine::Basic => Kind::Folder,
                "dock" if self.engine != Engine::Basic => Kind::Dock,
                "dropdown" if self.engine != Engine::Basic => Kind::Dropdown,
                "color" if self.engine != Engine::Basic => Kind::Color,
                _ => return None,
            },
            _ => return None,
        };
        Some(self.with_kind(kind))
    }

    pub(super) fn field(self, name: &str) -> Option<Self> {
        let signal = match name {
            "closed" => self.kind == Kind::Window,
            "pressed" | "hovered" | "unhovered" => self.kind == Kind::Button,
            "changed" => self.has_value() && self.kind != Kind::Label,
            _ => false,
        };
        signal.then_some(self.with_kind(Kind::Signal))
    }

    fn has_method(self, name: &str) -> bool {
        if self.result(name).is_some() {
            return true;
        }
        match name {
            "show" | "hide" | "close" => self.kind == Kind::Window,
            "collapse" | "expand" | "is_collapsed" | "set_position" => {
                self.kind == Kind::Window && self.engine != Engine::Basic
            }
            "get" | "set" => self.has_value(),
            "switch" => matches!(self.kind, Kind::Toggle | Kind::Folder),
            "select" => self.kind == Kind::Tab,
            "add_option" | "remove_option" => self.kind == Kind::Dropdown,
            "set_color" | "get_color" | "destroy" => self.control() && self.engine != Engine::Basic,
            "disconnect" => self.kind == Kind::Connection,
            _ => false,
        }
    }

    pub(super) fn suggestions(self, include_fields: bool) -> Vec<Item> {
        let methods = [
            ENGINE_METHODS,
            WINDOW_METHODS,
            CONTROL_METHODS,
            SIGNAL_METHODS,
            CONNECTION_METHODS,
        ]
        .into_iter()
        .flatten()
        .filter(|item| self.has_method(&item.name));
        let fields = SIGNAL_FIELDS
            .iter()
            .filter(|item| include_fields && self.field(&item.name).is_some());
        methods.chain(fields).cloned().collect()
    }
}
