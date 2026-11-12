use std::{
    io::{self, BufRead, BufReader, Write},
    os::fd::{AsRawFd, RawFd},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use base64::{engine::general_purpose::STANDARD, Engine};

use super::{Control, ControlValue, Event, Gui, Kind, Update, Window};
use crate::linux::Keyboard;

pub struct TkWindow {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    keyboard: Option<Keyboard>,
}

impl TkWindow {
    pub fn new(window: &Window) -> io::Result<Self> {
        let keyboard = if window
            .controls
            .iter()
            .any(|control| control.kind == Kind::Keybind)
        {
            Some(Keyboard::for_capture().map_err(|error| io::Error::other(error.to_string()))?)
        } else {
            None
        };
        let mut script = tempfile::NamedTempFile::new()?;
        script.write_all(include_bytes!("tk.tcl"))?;
        let mut child = Command::new("wish")
            .arg(script.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    io::Error::other(
                        "The Basic GUI engine needs Tk. Install the 'tk' package to use it.",
                    )
                } else {
                    error
                }
            })?;
        let input = child.stdin.take().expect("piped stdin");
        let output = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut native = Self {
            child,
            input,
            output,
            keyboard,
        };
        let mut ready = String::new();
        native.output.read_line(&mut ready)?;
        if ready.trim_end() != "ready" {
            return Err(io::Error::other("Tk could not open the GUI window."));
        }
        writeln!(native.input, "lhk::title {}", encode(&window.title))?;
        for (id, control) in window.controls.iter().enumerate() {
            native.add(id, control)?;
        }
        writeln!(native.input, "lhk::layout")?;
        native.visible(window.visible)?;
        Ok(native)
    }

    fn add(&mut self, id: usize, control: &Control) -> io::Result<()> {
        let label = encode(&control.label);
        let value = encoded_value(&control.value);
        match control.kind {
            Kind::Label => writeln!(self.input, "lhk::add label {id} {value}"),
            Kind::Button => writeln!(self.input, "lhk::add button {id} {label}"),
            Kind::Slider { min, max } => {
                writeln!(
                    self.input,
                    "lhk::add slider {id} {label} {min} {max} {value}"
                )
            }
            Kind::Toggle => writeln!(self.input, "lhk::add toggle {id} {label} {value}"),
            Kind::Input => writeln!(self.input, "lhk::add input {id} {label} {value}"),
            Kind::Keybind => writeln!(
                self.input,
                "lhk::add keybind {id} {}",
                encode(&control.keybind_caption(false))
            ),
        }
    }

    fn visible(&mut self, visible: bool) -> io::Result<()> {
        writeln!(self.input, "lhk::visible {}", u8::from(visible))
    }

    pub fn fd(&self) -> RawFd {
        self.output.get_ref().as_raw_fd()
    }

    pub fn read(&mut self) -> io::Result<Option<Event>> {
        loop {
            if self.output.buffer().is_empty() && !wait_readable(&[self.fd()], 0)? {
                return Ok(None);
            }
            let mut line = String::new();
            if self.output.read_line(&mut line)? == 0 {
                return Err(io::Error::other("The Tk GUI stopped unexpectedly."));
            }
            let mut parts = line.trim_end_matches(['\r', '\n']).splitn(3, ' ');
            let event = parts.next().unwrap_or("");
            if event == "close" {
                return Ok(Some(Event::Close));
            }
            if event == "error" {
                let message = decode(parts.next().unwrap_or(""))?;
                return Err(io::Error::other(format!("Tk GUI: {message}")));
            }
            let id = parts
                .next()
                .and_then(|id| id.parse().ok())
                .ok_or_else(protocol_error)?;
            let value = parts.next().unwrap_or("");
            let event =
                match event {
                    "pressed" => Event::Press(id),
                    "hovered" => Event::Hover(id, true),
                    "unhovered" => Event::Hover(id, false),
                    "record" => Event::Record(id),
                    "cancel" => Event::CancelRecording(id),
                    "key" | "release" => {
                        let (code, modifiers) = value.split_once(' ').ok_or_else(protocol_error)?;
                        let code = code.parse().map_err(|_| protocol_error())?;
                        let modifiers = modifiers.parse().map_err(|_| protocol_error())?;
                        let Some(combo) = self.keyboard.as_ref().and_then(|keyboard| {
                            keyboard.capture(code, modifiers, event == "release")
                        }) else {
                            continue;
                        };
                        Event::Key(id, combo)
                    }
                    "number" => Event::Change(
                        id,
                        ControlValue::Number(value.parse().map_err(|_| protocol_error())?),
                    ),
                    "boolean" => Event::Change(id, ControlValue::Boolean(value == "1")),
                    "text" => Event::Change(id, ControlValue::Text(decode(value)?)),
                    _ => return Err(protocol_error()),
                };
            return Ok(Some(event));
        }
    }

    pub fn sync(&mut self, gui: &Gui) -> io::Result<()> {
        let updates = gui.updates();
        let window = gui.window();
        let window = window.as_ref().expect("Tk GUI");
        for update in updates {
            match update {
                Update::Close => break,
                Update::Visibility(visible) => self.visible(visible)?,
                Update::Control(id) => {
                    let control = &window.controls[id];
                    if control.kind == Kind::Keybind {
                        let recording = window.recording == Some(id);
                        writeln!(
                            self.input,
                            "lhk::set_keybind {id} {} {}",
                            encode(&control.keybind_caption(recording)),
                            u8::from(recording)
                        )?;
                        continue;
                    }
                    let kind = match control.kind {
                        Kind::Label => "label",
                        Kind::Input => "text",
                        _ => "value",
                    };
                    writeln!(
                        self.input,
                        "lhk::set_value {kind} {id} {}",
                        encoded_value(&control.value)
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl Drop for TkWindow {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn protocol_error() -> io::Error {
    io::Error::other("Unexpected message from the Tk GUI.")
}

fn encode(value: &str) -> String {
    format!("{{{}}}", STANDARD.encode(value))
}

fn decode(value: &str) -> io::Result<String> {
    let bytes = STANDARD.decode(value).map_err(|_| protocol_error())?;
    String::from_utf8(bytes).map_err(|_| protocol_error())
}

fn encoded_value(value: &ControlValue) -> String {
    match value {
        ControlValue::None => String::new(),
        ControlValue::Text(value) => encode(value),
        ControlValue::Number(value) => value.to_string(),
        ControlValue::Boolean(value) => u8::from(*value).to_string(),
    }
}

pub fn wait_readable(descriptors: &[RawFd], timeout: i32) -> io::Result<bool> {
    let mut poll = descriptors
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect::<Vec<_>>();
    loop {
        let count = unsafe { libc::poll(poll.as_mut_ptr(), poll.len() as libc::nfds_t, timeout) };
        if count >= 0 {
            return Ok(count != 0);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
