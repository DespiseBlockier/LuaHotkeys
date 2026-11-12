use imgui::{Condition, FontId, MouseButton, Style, StyleColor, StyleVar, Ui, WindowFlags};
use winit::window::Window as HostWindow;

use super::TITLE_HEIGHT;
use crate::macro_gui::{rgba, Control, ControlValue, Event, Gui, Kind, Theme, Window};

const TAB_HEIGHT: f32 = 30.0;

fn blend(from: [f32; 4], to: [f32; 4], amount: f32) -> [f32; 4] {
    std::array::from_fn(|i| from[i] * (1.0 - amount) + to[i] * amount)
}

pub(super) fn apply_theme(style: &mut Style, theme: Theme) {
    style.window_rounding = theme.rounding;
    style.window_min_size = [1.0, 1.0];
    style.window_border_size = 0.0;
    style.child_border_size = 0.0;
    style.frame_rounding = theme.rounding.min(8.0);
    style.popup_rounding = style.frame_rounding;
    style.popup_border_size = 0.0;
    style.scrollbar_rounding = 3.0;
    style.scrollbar_size = 5.0;
    style.grab_rounding = style.frame_rounding;
    style.frame_padding = [10.0, 3.0];
    style.item_spacing = [8.0, 5.0];
    style.item_inner_spacing = [8.0, 4.0];
    for (color, value) in [
        (StyleColor::Text, theme.text),
        (StyleColor::WindowBg, theme.background),
        (StyleColor::PopupBg, theme.background),
        (StyleColor::ChildBg, [0.0; 4]),
        (StyleColor::FrameBg, theme.tabs),
        (
            StyleColor::FrameBgHovered,
            blend(theme.tabs, theme.control, 0.20),
        ),
        (
            StyleColor::FrameBgActive,
            blend(theme.tabs, theme.control, 0.35),
        ),
        (StyleColor::Button, theme.control),
        (
            StyleColor::ButtonHovered,
            blend(theme.control, theme.text, 0.17),
        ),
        (
            StyleColor::ButtonActive,
            blend(theme.control, theme.background, 0.24),
        ),
        (StyleColor::Header, theme.tabs),
        (
            StyleColor::HeaderHovered,
            blend(theme.tabs, theme.control, 0.35),
        ),
        (StyleColor::HeaderActive, theme.control),
        (StyleColor::CheckMark, theme.accent),
        (StyleColor::SliderGrab, theme.accent),
        (StyleColor::SliderGrabActive, theme.accent),
        (StyleColor::ScrollbarGrab, theme.control),
        (StyleColor::ScrollbarGrabHovered, theme.accent),
        (StyleColor::ScrollbarGrabActive, theme.accent),
        (StyleColor::NavHighlight, theme.accent),
    ] {
        style[color] = value;
    }
}

fn chevron(ui: &Ui, center: [f32; 2], open: bool, color: [f32; 4]) {
    let [x, y] = center;
    let [start, middle, end] = if open {
        [[x - 4.0, y - 2.0], [x, y + 2.0], [x + 4.0, y - 2.0]]
    } else {
        [[x - 2.0, y - 4.0], [x + 2.0, y], [x - 2.0, y + 4.0]]
    };
    let draw = ui.get_window_draw_list();
    draw.add_line(start, middle, color).thickness(1.6).build();
    draw.add_line(middle, end, color).thickness(1.6).build();
}

pub(super) struct PanelPaint {
    title_font: FontId,
    search: String,
    dropdown: Option<usize>,
    events: Vec<Event>,
    hovered: Option<usize>,
    recording_hovered: bool,
}

impl PanelPaint {
    pub(super) fn new(title_font: FontId) -> Self {
        Self {
            title_font,
            search: String::new(),
            dropdown: None,
            events: Vec::new(),
            hovered: None,
            recording_hovered: false,
        }
    }

    fn header(&mut self, ui: &Ui, window: &Window, host: &HostWindow) {
        let [width, height] = ui.io().display_size;
        let title_height = TITLE_HEIGHT as f32;
        let theme = window.appearance.theme;
        let draw = ui.get_window_draw_list();
        draw.add_rect([0.0, 0.0], [width, title_height], theme.header)
            .rounding(theme.rounding.min(title_height / 2.0))
            .round_bot_left(height <= title_height + 1.0)
            .round_bot_right(height <= title_height + 1.0)
            .filled(true)
            .build();
        {
            let _font = ui.push_font(self.title_font);
            draw.with_clip_rect([30.0, 0.0], [width - 5.0, title_height], || {
                draw.add_text(
                    [30.0, (title_height - ui.text_line_height()) * 0.5],
                    theme.text,
                    &window.title,
                );
            });
        }
        drop(draw);
        ui.set_cursor_pos([26.0, 0.0]);
        ui.invisible_button("##drag", [(width - 26.0).max(1.0), title_height]);
        if ui.is_item_clicked() {
            let _ = host.drag_window();
        }
        ui.set_cursor_pos([4.0, 1.0]);
        let _normal = ui.push_style_color(StyleColor::Button, [0.0; 4]);
        let _hover = ui.push_style_color(StyleColor::ButtonHovered, [1.0, 1.0, 1.0, 0.16]);
        let _active = ui.push_style_color(StyleColor::ButtonActive, [0.0, 0.0, 0.0, 0.15]);
        if ui.button_with_size("##collapse", [20.0, 20.0]) {
            self.events.push(Event::Collapse(!window.collapsed));
        }
        chevron(ui, [14.0, 11.0], !window.collapsed, theme.text);
    }

    pub(super) fn draw(&mut self, ui: &Ui, gui: &Gui, host: &HostWindow) {
        self.hovered = None;
        self.recording_hovered = false;
        let window = gui.window();
        let panel = window.as_ref().expect("Imgui GUI");
        ui.window("##macro")
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
                self.header(ui, panel, host);
                let [width, height] = ui.io().display_size;
                if height > TITLE_HEIGHT as f32 + 3.0 {
                    let mut top = TITLE_HEIGHT as f32;
                    if panel.active_tab.is_some() {
                        self.tabs(ui, panel, width, top);
                        top += TAB_HEIGHT;
                    }
                    if height > top + 7.0 {
                        ui.set_cursor_pos([10.0, top + 6.0]);
                        ui.child_window("##controls")
                            .size([(width - 20.0).max(1.0), (height - top - 11.0).max(1.0)])
                            .build(|| {
                                for (id, control) in panel.controls.iter().enumerate() {
                                    if control.parent.is_none()
                                        && control.kind != Kind::Tab
                                        && !control.removed
                                    {
                                        self.control(ui, panel, id);
                                    }
                                }
                                if let Some(id) = panel.active_tab {
                                    for &child in &panel.controls[id].children {
                                        self.control(ui, panel, child);
                                    }
                                }
                            });
                    }
                }
            });
        if let Some(id) = panel.recording {
            if ui.is_mouse_clicked(MouseButton::Left) && !self.recording_hovered {
                self.events.insert(0, Event::CancelRecording(id));
            }
        }
        for (id, control) in panel.controls.iter().enumerate() {
            let hovered = self.hovered == Some(id);
            if control.kind == Kind::Button && !control.removed && control.hovered != hovered {
                self.events.push(Event::Hover(id, hovered));
            }
        }
        drop(window);
        for event in self.events.drain(..) {
            gui.event(event);
        }
    }

    fn tabs(&mut self, ui: &Ui, window: &Window, width: f32, top: f32) {
        let theme = window.appearance.theme;
        let draw = ui.get_window_draw_list();
        draw.add_rect([0.0, top], [width, top + TAB_HEIGHT], theme.tabs)
            .filled(true)
            .build();
        draw.add_rect(
            [0.0, top + TAB_HEIGHT - 2.0],
            [width, top + TAB_HEIGHT],
            blend(theme.tabs, theme.background, 0.55),
        )
        .filled(true)
        .build();
        drop(draw);
        ui.set_cursor_pos([15.0, top]);
        ui.child_window("##tabs")
            .size([(width - 15.0).max(1.0), TAB_HEIGHT - 2.0])
            .build(|| {
                let _font = ui.push_font(self.title_font);
                let _padding = ui.push_style_var(StyleVar::FramePadding([0.0, 3.0]));
                let _normal = ui.push_style_color(StyleColor::Button, [0.0; 4]);
                let _hover = ui.push_style_color(StyleColor::ButtonHovered, [1.0, 1.0, 1.0, 0.10]);
                let _active = ui.push_style_color(StyleColor::ButtonActive, [1.0, 1.0, 1.0, 0.15]);
                let mut first = true;
                for (id, tab) in window.controls.iter().enumerate() {
                    if tab.kind != Kind::Tab || tab.removed {
                        continue;
                    }
                    if !first {
                        ui.same_line_with_spacing(0.0, 15.0);
                    }
                    first = false;
                    let _id = ui.push_id_usize(id);
                    let color = tab.tint.map_or(theme.text, rgba);
                    let _text = ui.push_style_color(
                        StyleColor::Text,
                        if window.active_tab == Some(id) {
                            color
                        } else {
                            blend(color, theme.tabs, 0.48)
                        },
                    );
                    let [width, _] = ui.calc_text_size(&tab.label);
                    if ui.button_with_size(&tab.label, [width + 1.0, TAB_HEIGHT - 2.0]) {
                        self.events.push(Event::SelectTab(id));
                    }
                }
            });
    }

    fn control(&mut self, ui: &Ui, window: &Window, id: usize) {
        let control = &window.controls[id];
        let _id = ui.push_id_usize(id);
        let theme = window.appearance.theme;
        let tint = control.tint.map_or(theme.control, rgba);
        match (control.kind, &control.value) {
            (Kind::Label, ControlValue::Text(value)) => {
                let _color =
                    ui.push_style_color(StyleColor::Text, control.tint.map_or(theme.text, rgba));
                ui.text_wrapped(value);
            }
            (Kind::Folder, ControlValue::Boolean(open)) => {
                let _bg = ui.push_style_color(StyleColor::Button, tint);
                let _hover =
                    ui.push_style_color(StyleColor::ButtonHovered, blend(tint, theme.text, 0.15));
                let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.0, 0.5]));
                let label = format!("      {}###folder", control.label);
                if ui.button_with_size(label, [ui.content_region_avail()[0].max(1.0), 20.0]) {
                    self.events
                        .push(Event::Change(id, ControlValue::Boolean(!open)));
                }
                let [x, y] = ui.item_rect_min();
                chevron(ui, [x + 12.0, y + 10.0], *open, theme.text);
                if *open {
                    ui.indent_by(10.0);
                    for &child in &control.children {
                        self.control(ui, window, child);
                    }
                    ui.unindent_by(10.0);
                }
            }
            (Kind::Dock, _) => {
                let _group = ui.begin_group();
                for (index, &child) in control.children.iter().enumerate() {
                    if index > 0 {
                        ui.same_line();
                    }
                    let _item = ui.begin_group();
                    self.control(ui, window, child);
                }
            }
            (Kind::Button, _) | (Kind::Keybind, _) => {
                let recording = window.recording == Some(id);
                let caption;
                let label = if control.kind == Kind::Keybind {
                    caption = format!("{}###record", control.keybind_caption(recording));
                    caption.as_str()
                } else {
                    control.label.as_str()
                };
                let [x, y] = ui.cursor_screen_pos();
                let width = ui.calc_text_size_with_opts(label, true, -1.0)[0] + 20.0;
                ui.get_window_draw_list()
                    .add_rect(
                        [x + 2.0, y + 2.0],
                        [x + width + 2.0, y + ui.frame_height() + 2.0],
                        blend(tint, [0.0, 0.0, 0.0, 1.0], 0.50),
                    )
                    .rounding(theme.rounding.min(8.0))
                    .filled(true)
                    .build();
                let _normal = ui.push_style_color(StyleColor::Button, tint);
                let _hover =
                    ui.push_style_color(StyleColor::ButtonHovered, blend(tint, theme.text, 0.15));
                let _active = ui.push_style_color(
                    StyleColor::ButtonActive,
                    blend(tint, theme.background, 0.25),
                );
                if control.kind == Kind::Button {
                    if ui.button(label) {
                        self.events.push(Event::Press(id));
                    }
                    if ui.is_item_hovered() {
                        self.hovered = Some(id);
                    }
                } else {
                    if ui.button(label) {
                        self.events.push(Event::Record(id));
                    }
                    if recording {
                        self.recording_hovered = ui.is_item_hovered();
                    }
                }
            }
            (Kind::Slider { min, max }, ControlValue::Number(value)) => {
                let width = 150.0_f32.min(ui.content_region_avail()[0].max(1.0));
                let _width = ui.push_item_width(width);
                let start = ui.cursor_screen_pos();
                let progress = (*value - min) as f32 / (max - min) as f32;
                let end = [start[0] + width, start[1] + ui.frame_height()];
                let draw = ui.get_window_draw_list();
                let rounding = theme.rounding.min(8.0);
                draw.add_rect(start, end, tint)
                    .rounding(rounding)
                    .filled(true)
                    .build();
                draw.with_clip_rect(start, [start[0] + width * progress, end[1]], || {
                    draw.add_rect(start, end, blend(tint, theme.text, 0.30))
                        .rounding(rounding)
                        .filled(true)
                        .build();
                });
                let _base = ui.push_style_color(StyleColor::FrameBg, [0.0; 4]);
                let _hover = ui.push_style_color(StyleColor::FrameBgHovered, [1.0, 1.0, 1.0, 0.10]);
                let _active = ui.push_style_color(StyleColor::FrameBgActive, [0.0, 0.0, 0.0, 0.10]);
                let _grab = ui.push_style_color(StyleColor::SliderGrab, [0.0; 4]);
                let _grab_active = ui.push_style_color(StyleColor::SliderGrabActive, [0.0; 4]);
                let mut value = i64::from(*value);
                if ui.slider("##value", i64::from(min), i64::from(max), &mut value) {
                    self.events
                        .push(Event::Change(id, ControlValue::Number(value as i32)));
                }
                ui.same_line();
                ui.text(&control.label);
            }
            (Kind::Toggle, ControlValue::Boolean(value)) => {
                let _frame = ui.push_style_color(StyleColor::FrameBg, tint);
                let _mark = ui.push_style_color(StyleColor::CheckMark, theme.text);
                let mut value = *value;
                if ui.checkbox(&control.label, &mut value) {
                    self.events
                        .push(Event::Change(id, ControlValue::Boolean(value)));
                }
            }
            (Kind::Input, ControlValue::Text(value)) => {
                let _width = ui.push_item_width(150.0);
                let _frame = control
                    .tint
                    .map(|color| ui.push_style_color(StyleColor::FrameBg, rgba(color)));
                let mut value = value.clone();
                if ui.input_text(&control.label, &mut value).build() {
                    self.events
                        .push(Event::Change(id, ControlValue::Text(value)));
                }
            }
            (Kind::Dropdown, value) => self.dropdown(
                ui,
                control,
                id,
                value,
                control.tint.map_or(theme.dropdown, rgba),
            ),
            (Kind::Color, ControlValue::Color(rgb)) => {
                let [r, g, b, _] = rgba(*rgb);
                let mut color = [r, g, b];
                let _frame = control
                    .tint
                    .map(|color| ui.push_style_color(StyleColor::FrameBg, rgba(color)));
                if ui
                    .color_edit3_config(&control.label, &mut color)
                    .inputs(false)
                    .build()
                {
                    let [r, g, b] =
                        color.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u32);
                    self.events.push(Event::Change(
                        id,
                        ControlValue::Color((r << 16) | (g << 8) | b),
                    ));
                }
            }
            _ => unreachable!("GUI control value"),
        }
    }

    fn dropdown(
        &mut self,
        ui: &Ui,
        control: &Control,
        id: usize,
        value: &ControlValue,
        tint: [f32; 4],
    ) {
        let _width = ui.push_item_width(150.0);
        let _frame = ui.push_style_color(StyleColor::FrameBg, tint);
        let selected = match value {
            ControlValue::Text(value) => value,
            _ => "Select...",
        };
        if let Some(_popup) = ui.begin_combo(&control.label, selected) {
            if self.dropdown != Some(id) || ui.is_window_appearing() {
                self.dropdown = Some(id);
                self.search.clear();
                ui.set_keyboard_focus_here();
            }
            ui.input_text("##search", &mut self.search)
                .hint("Search...")
                .build();
            let query = self.search.to_lowercase();
            for (index, option) in control.options.iter().enumerate() {
                if option.to_lowercase().contains(&query) {
                    let _id = ui.push_id_usize(index);
                    if ui
                        .selectable_config(option)
                        .selected(option == selected)
                        .build()
                    {
                        self.events
                            .push(Event::Change(id, ControlValue::Text(option.clone())));
                    }
                }
            }
        }
    }
}
