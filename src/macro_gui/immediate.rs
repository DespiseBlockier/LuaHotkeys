use std::{
    error::Error,
    io,
    num::NonZeroU32,
    time::{Duration, Instant},
};

use glutin::{
    config::ConfigTemplateBuilder,
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
    },
    display::{GetGlDisplay, GlDisplay},
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use imgui::{Condition, ConfigFlags, MouseButton, StyleColor, WindowFlags};
use imgui_glow_renderer::{
    glow::{self, HasContext},
    AutoRenderer,
};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use mlua::Lua;
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{Event as WindowEventLoop, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    keyboard::{Key as LogicalKey, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    platform::modifier_supplement::KeyEventExtModifierSupplement,
    raw_window_handle::HasWindowHandle,
    window::{Window as HostWindow, WindowAttributes},
};

use super::{ControlValue, Event, Gui, Kind, Theme, Window};
use crate::{
    runtime::Output,
    shortcut::{self, Combination, Key, ModifierKey, NumpadOperator, ALT, CONTROL, SHIFT, SUPER},
};

pub fn run(
    lua: &Lua,
    gui: &Gui,
    output: Output,
    mut builder: EventLoopBuilder<()>,
    mut hotkeys: impl FnMut() -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let event_loop = builder.build()?;
    let host = {
        let window = gui.window();
        Host::new(&event_loop, window.as_ref().expect("Imgui GUI"))?
    };
    let theme = gui.window().as_ref().expect("Imgui GUI").appearance.theme;
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    apply_theme(imgui.style_mut(), theme);
    imgui
        .io_mut()
        .config_flags
        .insert(ConfigFlags::NAV_ENABLE_KEYBOARD);
    let mut platform = WinitPlatform::new(&mut imgui);
    platform.attach_window(imgui.io_mut(), &host.window, HiDpiMode::Default);
    let mut renderer = AutoRenderer::new(host.glow(), &mut imgui)?;
    let mut last_frame = Instant::now();
    let mut next_frame = last_frame;
    let mut visible = gui.window().as_ref().is_some_and(|window| window.visible);
    host.window.set_visible(visible);
    let mut occluded = false;
    let mut rounded_size = host.window.inner_size();
    let mut modifiers = ModifiersState::empty();
    let mut failure = None;

    #[allow(deprecated)]
    event_loop.run(|event, target| {
        let about_to_wait = matches!(event, WindowEventLoop::AboutToWait);
        let recording = gui.recording();
        let recording_input = recording.is_some()
            && matches!(
                &event,
                WindowEventLoop::WindowEvent { event: WindowEvent::KeyboardInput { event, .. }, .. }
                    if event.state.is_pressed()
            );
        if !recording_input {
            platform.handle_event(imgui.io_mut(), &host.window, &event);
        }
        let result = (|| -> Result<(), Box<dyn Error>> {
            match event {
                WindowEventLoop::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => gui.event(Event::Close),
                    WindowEvent::ModifiersChanged(state) => modifiers = state.state(),
                    WindowEvent::Focused(false) => {
                        if let Some(id) = recording {
                            gui.event(Event::CancelRecording(id));
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. }
                        if recording.is_some() && !event.repeat =>
                    {
                        if let (Some(id), Some(combo)) = (recording, capture_key(&event, modifiers))
                        {
                            gui.event(Event::Key(id, combo));
                        }
                    }
                    WindowEvent::Occluded(value) => occluded = value,
                    WindowEvent::Resized(size) => {
                        if let (Some(width), Some(height)) =
                            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                        {
                            host.surface.resize(&host.context, width, height);
                            if size != rounded_size {
                                round_window(&host.window, size, theme.rounding);
                                rounded_size = size;
                            }
                        }
                    }
                    WindowEvent::RedrawRequested if visible && !occluded => {
                        let now = Instant::now();
                        imgui
                            .io_mut()
                            .update_delta_time(now.duration_since(last_frame));
                        last_frame = now;
                        platform.prepare_frame(imgui.io_mut(), &host.window)?;
                        let ui = imgui.frame();
                        draw(ui, gui, &host.window);
                        platform.prepare_render(ui, &host.window);
                        unsafe {
                            let [r, g, b, a] = theme.background;
                            renderer.gl_context().clear_color(r, g, b, a);
                            renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
                        }
                        renderer.render(imgui.render())?;
                        host.surface.swap_buffers(&host.context)?;
                    }
                    _ => {}
                },
                WindowEventLoop::AboutToWait => hotkeys()?,
                _ => {}
            }
            gui.dispatch(lua, output);
            gui.updates();
            let window = gui.window();
            let window = window.as_ref().expect("Imgui GUI");
            if window.closed {
                target.exit();
                return Ok(());
            }
            if visible != window.visible {
                visible = window.visible;
                host.window.set_visible(visible);
                next_frame = Instant::now();
            }
            if about_to_wait {
                let now = Instant::now();
                if now >= next_frame {
                    let interval = if visible && !occluded {
                        host.window.request_redraw();
                        Duration::from_millis(16)
                    } else {
                        Duration::from_millis(32)
                    };
                    next_frame = now + interval;
                }
                target.set_control_flow(ControlFlow::WaitUntil(next_frame));
            }
            Ok(())
        })();
        if let Err(error) = result {
            failure = Some(error);
            target.exit();
        }
    })?;
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Host {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    window: HostWindow,
}

impl Host {
    fn new(event_loop: &EventLoop<()>, window: &Window) -> Result<Self, Box<dyn Error>> {
        let auto_height = (142 + window.controls.len().saturating_mul(44)).clamp(180, 650);
        let size = LogicalSize::new(
            window.appearance.width.unwrap_or(460),
            window.appearance.height.unwrap_or(auto_height as u32),
        );
        let rounding = window.appearance.theme.rounding;
        let attributes = WindowAttributes::default()
            .with_title(&window.title)
            .with_inner_size(size)
            .with_resizable(false)
            .with_decorations(false)
            .with_visible(false);
        let (window, config) = glutin_winit::DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(
                event_loop,
                ConfigTemplateBuilder::new().with_alpha_size(8),
                |mut configs| configs.next().expect("OpenGL display configuration"),
            )?;
        let window =
            window.ok_or_else(|| io::Error::other("Could not create the Dear ImGui window."))?;
        let handle = window.window_handle()?.as_raw();
        let display = config.display();
        let context = unsafe {
            display
                .create_context(
                    &config,
                    &ContextAttributesBuilder::new()
                        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
                        .build(Some(handle)),
                )
                .or_else(|_| {
                    display.create_context(
                        &config,
                        &ContextAttributesBuilder::new()
                            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
                            .build(Some(handle)),
                    )
                })
        }
        .map_err(|error| {
            io::Error::other(format!(
                "The Imgui engine could not open an OpenGL context. {error}"
            ))
        })?;
        let size = window.inner_size();
        round_window(&window, size, rounding);
        let surface = unsafe {
            display.create_window_surface(
                &config,
                &SurfaceAttributesBuilder::<WindowSurface>::new().build(
                    handle,
                    NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                    NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
                ),
            )?
        };
        let context = context.make_current(&surface)?;
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN));
        Ok(Self {
            window,
            surface,
            context,
        })
    }

    fn glow(&self) -> glow::Context {
        unsafe {
            glow::Context::from_loader_function_cstr(|name| {
                self.context.display().get_proc_address(name).cast()
            })
        }
    }
}

fn apply_theme(style: &mut imgui::Style, theme: Theme) {
    style.window_rounding = theme.rounding;
    style.window_border_size = 0.0;
    style.child_border_size = 0.0;
    style.frame_rounding = theme.rounding.min(8.0);
    style.grab_rounding = style.frame_rounding;
    style.scrollbar_rounding = style.frame_rounding;
    style.frame_padding = [8.0, 6.0];
    style.item_spacing = [8.0, 8.0];
    let blend = |strength: f32| {
        std::array::from_fn(|index| {
            theme.control[index] * (1.0 - strength) + theme.accent[index] * strength
        })
    };
    for (color, value) in [
        (StyleColor::Text, theme.text),
        (StyleColor::WindowBg, theme.background),
        (StyleColor::ChildBg, theme.background),
        (StyleColor::FrameBg, theme.control),
        (StyleColor::FrameBgHovered, blend(0.18)),
        (StyleColor::FrameBgActive, blend(0.28)),
        (StyleColor::Button, theme.control),
        (StyleColor::ButtonHovered, blend(0.23)),
        (StyleColor::ButtonActive, blend(0.35)),
        (StyleColor::CheckMark, theme.accent),
        (StyleColor::SliderGrab, theme.accent),
        (StyleColor::SliderGrabActive, theme.accent),
        (StyleColor::NavHighlight, theme.accent),
    ] {
        style[color] = value;
    }
}

#[cfg(windows)]
fn round_window(window: &HostWindow, size: PhysicalSize<u32>, rounding: f32) {
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    use winit::raw_window_handle::RawWindowHandle;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let handle = handle.hwnd.get() as _;
    let diameter = (2.0 * rounding * window.scale_factor() as f32).round() as i32;
    unsafe {
        if diameter == 0 {
            SetWindowRgn(handle, std::ptr::null_mut(), 1);
            return;
        }
        let region = CreateRoundRectRgn(
            0,
            0,
            size.width as i32 + 1,
            size.height as i32 + 1,
            diameter,
            diameter,
        );
        if !region.is_null() && SetWindowRgn(handle, region, 1) == 0 {
            DeleteObject(region);
        }
    }
}

#[cfg(target_os = "linux")]
fn round_window(window: &HostWindow, size: PhysicalSize<u32>, rounding: f32) {
    use winit::raw_window_handle::RawWindowHandle;
    use x11rb::protocol::{
        shape::{ConnectionExt, SK, SO},
        xproto::{ClipOrdering, Rectangle},
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let id = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return,
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let (Ok(width), Ok(height)) = (i16::try_from(size.width), i16::try_from(size.height)) else {
        return;
    };
    let radius =
        ((rounding * window.scale_factor() as f32).round() as i16).min(width.min(height) / 2);
    let mut rectangles = Vec::with_capacity(usize::from(radius as u16) * 2 + 1);
    for y in 0..radius {
        let distance = f32::from(radius - y) - 0.5;
        let inset = (f32::from(radius) - (f32::from(radius).powi(2) - distance.powi(2)).sqrt())
            .ceil() as i16;
        let row = Rectangle {
            x: inset,
            y,
            width: (width - 2 * inset) as u16,
            height: 1,
        };
        rectangles.push(row);
        rectangles.push(Rectangle {
            y: height - y - 1,
            ..row
        });
    }
    rectangles.push(Rectangle {
        x: 0,
        y: radius,
        width: width as u16,
        height: (height - 2 * radius) as u16,
    });
    let _ = connection
        .shape_rectangles(
            SO::SET,
            SK::BOUNDING,
            ClipOrdering::UNSORTED,
            id,
            0,
            0,
            &rectangles,
        )
        .map(|cookie| cookie.check());
}

const TITLE_HEIGHT: f32 = 40.0;

fn draw_header(ui: &imgui::Ui, window: &Window, host: &HostWindow) {
    let width = ui.io().display_size[0];
    let theme = window.appearance.theme;
    let draw = ui.get_window_draw_list();
    draw.add_rect([0.0, 0.0], [width, TITLE_HEIGHT], theme.header)
        .rounding(theme.rounding)
        .round_bot_left(false)
        .round_bot_right(false)
        .filled(true)
        .build();
    draw.with_clip_rect([14.0, 0.0], [width - 48.0, TITLE_HEIGHT], || {
        draw.add_text(
            [14.0, (TITLE_HEIGHT - ui.text_line_height()) * 0.5],
            theme.text,
            &window.title,
        );
    });
    ui.set_cursor_pos([0.0, 0.0]);
    ui.invisible_button("##drag_title", [width - 46.0, TITLE_HEIGHT]);
    if ui.is_item_clicked() {
        let _ = host.drag_window();
    }
    ui.set_cursor_pos([width - 38.0, 8.0]);
    if ui.button_with_size("##minimize", [28.0, 24.0]) {
        host.set_minimized(true);
    }
    let [x, y] = ui.item_rect_min();
    draw.add_line([x + 8.0, y + 13.0], [x + 20.0, y + 13.0], theme.text)
        .thickness(1.5)
        .build();
    if ui.is_item_hovered() {
        ui.tooltip_text("Minimize");
    }
}

fn draw(ui: &imgui::Ui, gui: &Gui, host: &HostWindow) {
    let mut events = Vec::new();
    {
        let window = gui.window();
        let window = window.as_ref().expect("Imgui GUI");
        let mut recorder_hovered = false;
        ui.window("##LuaHotkeysControls")
            .position([0.0, 0.0], Condition::Always)
            .size(ui.io().display_size, Condition::Always)
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_COLLAPSE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::NO_SCROLL_WITH_MOUSE
                    | WindowFlags::NO_SAVED_SETTINGS,
            )
            .build(|| {
                draw_header(ui, window, host);
                let [width, height] = ui.io().display_size;
                ui.set_cursor_pos([12.0, TITLE_HEIGHT + 12.0]);
                ui.child_window("##controls")
                    .size([
                        (width - 24.0).max(1.0),
                        (height - TITLE_HEIGHT - 24.0).max(1.0),
                    ])
                    .build(|| {
                        for (id, control) in window.controls.iter().enumerate() {
                            let _id = ui.push_id_usize(id);
                            match (control.kind, &control.value) {
                                (Kind::Label, ControlValue::Text(value)) => ui.text_wrapped(value),
                                (Kind::Button, _) => {
                                    let pressed = ui.button(&control.label);
                                    let hovered = ui.is_item_hovered();
                                    if hovered != control.hovered {
                                        events.push(Event::Hover(id, hovered));
                                    }
                                    if pressed {
                                        events.push(Event::Press(id));
                                    }
                                }
                                (Kind::Slider { min, max }, ControlValue::Number(value)) => {
                                    let mut value = i64::from(*value);
                                    if ui.slider(
                                        &control.label,
                                        i64::from(min),
                                        i64::from(max),
                                        &mut value,
                                    ) {
                                        events.push(Event::Change(
                                            id,
                                            ControlValue::Number(value as i32),
                                        ));
                                    }
                                }
                                (Kind::Toggle, ControlValue::Boolean(value)) => {
                                    let mut value = *value;
                                    if ui.checkbox(&control.label, &mut value) {
                                        events
                                            .push(Event::Change(id, ControlValue::Boolean(value)));
                                    }
                                }
                                (Kind::Input, ControlValue::Text(value)) => {
                                    let mut value = value.clone();
                                    if ui.input_text(&control.label, &mut value).build() {
                                        events.push(Event::Change(id, ControlValue::Text(value)));
                                    }
                                }
                                (Kind::Keybind, _) => {
                                    let recording = window.recording == Some(id);
                                    let caption =
                                        format!("{}###keybind", control.keybind_caption(recording));
                                    if ui.button(caption) {
                                        events.push(Event::Record(id));
                                    }
                                    if recording {
                                        recorder_hovered = ui.is_item_hovered();
                                    }
                                }
                                _ => unreachable!("GUI control value"),
                            }
                        }
                    });
            });
        if let Some(id) = window.recording {
            if ui.is_mouse_clicked(MouseButton::Left) && !recorder_hovered {
                events.insert(0, Event::CancelRecording(id));
            }
        }
    }
    for event in events {
        gui.event(event);
    }
}

fn capture_key(event: &winit::event::KeyEvent, modifiers: ModifiersState) -> Option<Combination> {
    const NUMPAD: [KeyCode; 10] = [
        KeyCode::Numpad0,
        KeyCode::Numpad1,
        KeyCode::Numpad2,
        KeyCode::Numpad3,
        KeyCode::Numpad4,
        KeyCode::Numpad5,
        KeyCode::Numpad6,
        KeyCode::Numpad7,
        KeyCode::Numpad8,
        KeyCode::Numpad9,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let position = |keys: &[KeyCode]| {
        keys.iter()
            .position(|&key| PhysicalKey::Code(key) == event.physical_key)
    };
    let physical = match event.physical_key {
        PhysicalKey::Code(KeyCode::SuperLeft) => Some(Key::Modifier(ModifierKey::LeftWin)),
        PhysicalKey::Code(KeyCode::SuperRight) => Some(Key::Modifier(ModifierKey::RightWin)),
        PhysicalKey::Code(KeyCode::NumpadDivide) => {
            Some(Key::NumpadOperator(NumpadOperator::Divide))
        }
        PhysicalKey::Code(KeyCode::NumpadMultiply) => {
            Some(Key::NumpadOperator(NumpadOperator::Multiply))
        }
        PhysicalKey::Code(KeyCode::NumpadSubtract) => {
            Some(Key::NumpadOperator(NumpadOperator::Minus))
        }
        PhysicalKey::Code(KeyCode::NumpadAdd) => Some(Key::NumpadOperator(NumpadOperator::Plus)),
        PhysicalKey::Code(KeyCode::NumpadEnter) => Some(Key::NumpadOperator(NumpadOperator::Enter)),
        PhysicalKey::Code(KeyCode::NumpadDecimal) => {
            Some(Key::NumpadOperator(NumpadOperator::Decimal))
        }
        _ => None,
    };
    let key = physical.or_else(|| {
        position(&NUMPAD)
            .map(|index| Key::Numpad(index as u16))
            .or_else(|| match event.key_without_modifiers() {
                LogicalKey::Character(value) => {
                    shortcut::parse(value.as_str()).ok().map(|combo| combo.key)
                }
                LogicalKey::Named(key) => named_key(key),
                _ => None,
            })
            .or_else(|| position(&DIGITS).map(|index| Key::Digit(b'0' + index as u8)))
    })?;
    if event.state.is_pressed() && matches!(key, Key::Modifier(_))
        || !event.state.is_pressed() && !matches!(key, Key::Modifier(_) | Key::PrintScreen)
    {
        return None;
    }
    let mut modifiers = [
        (modifiers.control_key(), CONTROL),
        (modifiers.alt_key(), ALT),
        (modifiers.shift_key(), SHIFT),
        (modifiers.super_key(), SUPER),
    ]
    .into_iter()
    .fold(
        0,
        |bits, (held, flag)| if held { bits | flag } else { bits },
    );
    if let Key::Modifier(key) = key {
        modifiers &= !key.flag();
    }
    Some(Combination { key, modifiers })
}

fn named_key(key: NamedKey) -> Option<Key> {
    use NamedKey::*;
    const FUNCTIONS: [NamedKey; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
        F21, F22, F23, F24,
    ];
    Some(match key {
        Enter => Key::Enter,
        Escape => Key::Escape,
        Space => Key::Space,
        Tab => Key::Tab,
        Backspace => Key::Backspace,
        Delete => Key::Delete,
        Insert => Key::Insert,
        CapsLock => Key::CapsLock,
        NumLock => Key::NumLock,
        ScrollLock => Key::ScrollLock,
        Pause => Key::Pause,
        ContextMenu => Key::Menu,
        Home => Key::Home,
        End => Key::End,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        ArrowUp => Key::Up,
        ArrowDown => Key::Down,
        ArrowLeft => Key::Left,
        ArrowRight => Key::Right,
        PrintScreen => Key::PrintScreen,
        AudioVolumeUp => Key::VolumeUp,
        AudioVolumeDown => Key::VolumeDown,
        AudioVolumeMute => Key::VolumeMute,
        MediaPlayPause => Key::MediaPlayPause,
        _ => Key::Function(FUNCTIONS.iter().position(|&function| function == key)? as u16 + 1),
    })
}
