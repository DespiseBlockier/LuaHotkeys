use std::{error::Error, num::NonZeroU32, time::Instant};

use glutin::surface::GlSurface;
use imgui::{Condition, ConfigFlags, StyleColor, WindowFlags};
use imgui_glow_renderer::{glow, glow::HasContext, AutoRenderer};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event as WindowEventLoop, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{WindowAttributes, WindowId},
};

use super::{activate, add_font, paint, round_window, Drag, Host, TITLE_HEIGHT};
use crate::macro_gui::{rgba, ControlValue, Event, Gui, Theme, Window};

const SIZE: LogicalSize<u32> = LogicalSize::new(230, 223);

pub(super) struct Palette {
    renderer: Option<AutoRenderer>,
    context: Option<imgui::SuspendedContext>,
    platform: WinitPlatform,
    host: Host,
    theme: Theme,
    drag: Drag,
    target: Option<usize>,
    title: String,
    last_frame: Instant,
}

impl Palette {
    pub(super) fn new(
        target: &ActiveEventLoop,
        parent: &Host,
        theme: Theme,
    ) -> Result<Self, Box<dyn Error>> {
        let attributes = WindowAttributes::default()
            .with_title("Color picker")
            .with_inner_size(SIZE)
            .with_resizable(false)
            .with_decorations(false)
            .with_visible(false);
        #[cfg(windows)]
        let attributes = {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes.with_skip_taskbar(true)
        };
        let window = glutin_winit::finalize_window(target, attributes, &parent.config)?;
        let host = Host::from_window(window, parent.config.clone(), SIZE, theme.rounding)?;
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);
        context
            .io_mut()
            .config_flags
            .insert(ConfigFlags::NAV_ENABLE_KEYBOARD);
        add_font(&mut context, 14.0);
        paint::apply_theme(context.style_mut(), theme);
        let mut platform = WinitPlatform::new(&mut context);
        platform.attach_window(context.io_mut(), &host.window, HiDpiMode::Default);
        let renderer = AutoRenderer::new(host.glow(), &mut context)?;
        Ok(Self {
            renderer: Some(renderer),
            context: Some(context.suspend()),
            platform,
            host,
            theme,
            drag: Drag::default(),
            target: None,
            title: String::new(),
            last_frame: Instant::now(),
        })
    }

    pub(super) fn id(&self) -> WindowId {
        self.host.window.id()
    }

    pub(super) fn show(&mut self, parent: &Host, gui: &Gui, id: usize) {
        let model = gui.window();
        let Some(control) = model
            .as_ref()
            .and_then(|window| window.controls.get(id))
            .filter(|control| !control.removed)
        else {
            return;
        };
        self.title.clone_from(&control.label);
        self.host.window.set_title(&self.title);
        if self.target.is_none() {
            if let Ok(origin) = parent.window.outer_position() {
                let size = parent.window.outer_size();
                let palette_size = SIZE.to_physical::<u32>(parent.window.scale_factor());
                let mut x = origin.x + size.width as i32 + 8;
                let mut y = origin.y;
                if let Some(monitor) = parent.window.current_monitor() {
                    let position = monitor.position();
                    let size = monitor.size();
                    let right = position.x + size.width as i32 - palette_size.width as i32;
                    if x > right {
                        x = origin.x - palette_size.width as i32 - 8;
                    }
                    x = x.clamp(position.x, right.max(position.x));
                    let bottom = position.y + size.height as i32 - palette_size.height as i32;
                    y = y.clamp(position.y, bottom.max(position.y));
                }
                self.host
                    .window
                    .set_outer_position(PhysicalPosition::new(x, y));
            }
            self.last_frame = Instant::now();
            self.host.window.set_visible(true);
        }
        self.target = Some(id);
        self.host.window.focus_window();
        self.host.window.request_redraw();
    }

    fn close(&mut self) {
        self.target = None;
        self.drag = Drag::default();
        self.host.window.set_visible(false);
    }

    pub(super) fn sync(&mut self, window: &Window) {
        if let Some(id) = self.target {
            if window.collapsed || !window.visible || window.controls[id].removed {
                self.close();
            }
        }
    }

    pub(super) fn redraw(&self) {
        if self.target.is_some() {
            self.host.window.request_redraw();
        }
    }

    pub(super) fn handle(
        &mut self,
        event: &WindowEventLoop<()>,
        gui: &Gui,
    ) -> Result<(), Box<dyn Error>> {
        let WindowEventLoop::WindowEvent { event: native, .. } = event else {
            return Ok(());
        };
        let mut context = activate(self.context.take().expect("palette context"))?;
        self.platform
            .handle_event(context.io_mut(), &self.host.window, event);
        let result = match native {
            WindowEvent::CloseRequested => {
                self.close();
                Ok(())
            }
            WindowEvent::Resized(size) => (|| {
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    self.host.make_current()?;
                    self.host.surface.resize(&self.host.context, width, height);
                    round_window(&self.host.window, *size, self.theme.rounding);
                }
                Ok(())
            })(),
            WindowEvent::RedrawRequested if self.target.is_some() => self.render(&mut context, gui),
            _ => Ok(()),
        };
        self.context = Some(context.suspend());
        result
    }

    fn render(&mut self, context: &mut imgui::Context, gui: &Gui) -> Result<(), Box<dyn Error>> {
        let id = self.target.expect("visible palette");
        let rgb = {
            let model = gui.window();
            model
                .as_ref()
                .and_then(|window| window.controls.get(id))
                .and_then(|control| {
                    if let ControlValue::Color(value) = control.value {
                        Some(value)
                    } else {
                        None
                    }
                })
        };
        let Some(rgb) = rgb else {
            self.close();
            return Ok(());
        };
        let [r, g, b, _] = rgba(rgb);
        let mut color = [r, g, b];
        let mut changed = false;
        let mut close = false;
        self.host.make_current()?;
        let now = Instant::now();
        context
            .io_mut()
            .update_delta_time(now.duration_since(self.last_frame));
        self.last_frame = now;
        self.platform
            .prepare_frame(context.io_mut(), &self.host.window)?;
        let ui = context.frame();
        let [width, height] = ui.io().display_size;
        let title_height = TITLE_HEIGHT as f32;
        ui.window("##palette")
            .position([0.0, 0.0], Condition::Always)
            .size([width, height], Condition::Always)
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::NO_SAVED_SETTINGS,
            )
            .build(|| {
                {
                    let draw = ui.get_window_draw_list();
                    draw.add_rect([0.0, 0.0], [width, title_height], self.theme.header)
                        .rounding(self.theme.rounding)
                        .round_bot_left(false)
                        .round_bot_right(false)
                        .filled(true)
                        .build();
                    draw.with_clip_rect([9.0, 0.0], [width - 29.0, title_height], || {
                        draw.add_text(
                            [9.0, (title_height - ui.text_line_height()) / 2.0],
                            self.theme.text,
                            &self.title,
                        );
                    });
                }
                ui.set_cursor_pos([0.0, 0.0]);
                ui.invisible_button("##drag", [(width - 27.0).max(1.0), title_height]);
                self.drag
                    .update(ui, &self.host.window, ui.is_item_clicked());
                ui.set_cursor_pos([width - 25.0, 1.0]);
                {
                    let _clear = ui.push_style_color(StyleColor::Button, [0.0; 4]);
                    close = ui.button_with_size("×", [22.0, 20.0]);
                }
                ui.set_cursor_pos([10.0, title_height + 10.0]);
                let _width = ui.push_item_width((width - 22.0).min(172.0).max(1.0));
                changed = ui
                    .color_picker3_config("##color", &mut color)
                    .inputs(false)
                    .options(false)
                    .side_preview(false)
                    .small_preview(false)
                    .build();
                let [r, g, b] =
                    color.map(|component| (component * 255.0).round().clamp(0.0, 255.0) as u32);
                ui.text(format!("#{r:02X}{g:02X}{b:02X}"));
                close |= ui.is_key_pressed(imgui::Key::Escape);
            });
        self.platform.prepare_render(ui, &self.host.window);
        let renderer = self.renderer.as_mut().expect("palette renderer");
        unsafe {
            let [r, g, b, a] = self.theme.background;
            renderer.gl_context().clear_color(r, g, b, a);
            renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
        }
        renderer.render(context.render())?;
        self.host.surface.swap_buffers(&self.host.context)?;
        if changed {
            let [r, g, b] =
                color.map(|component| (component * 255.0).round().clamp(0.0, 255.0) as u32);
            gui.event(Event::Change(
                id,
                ControlValue::Color((r << 16) | (g << 8) | b),
            ));
        }
        if close {
            self.close();
        }
        Ok(())
    }
}

impl Drop for Palette {
    fn drop(&mut self) {
        let _ = self.host.make_current();
        drop(self.renderer.take());
    }
}
