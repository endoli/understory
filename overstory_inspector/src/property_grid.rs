// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{boxed::Box, format, string::String, vec::Vec};

use overstory::{Color, ElementId, Panel, Row, TextBlock, Ui, peniko::color::palette};

/// One badge shown inside a property-grid row.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyBadge {
    /// Badge label.
    pub label: Box<str>,
    /// Badge background color.
    pub background: Color,
    /// Badge foreground color.
    pub foreground: Color,
}

impl PropertyBadge {
    /// Creates a new badge.
    #[must_use]
    pub fn new(label: impl Into<Box<str>>, background: Color, foreground: Color) -> Self {
        Self {
            label: label.into(),
            background,
            foreground,
        }
    }

    /// Creates a bright “active” badge using white foreground text.
    #[must_use]
    pub fn active(label: impl Into<Box<str>>, background: Color) -> Self {
        Self::new(label, background, palette::css::WHITE)
    }
}

/// Value payload for one property row.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// Plain text value.
    Text(Box<str>),
    /// Color value with a visible swatch.
    Color(Color),
    /// Badge/chip values.
    Badges(Vec<PropertyBadge>),
}

/// One property-grid row.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridRow {
    /// Property name.
    pub name: Box<str>,
    /// Property value payload.
    pub value: PropertyValue,
}

impl PropertyGridRow {
    /// Creates a new property row.
    #[must_use]
    pub fn new(name: impl Into<Box<str>>, value: PropertyValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// Element ids for one realized property-grid row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PropertyGridRowIds {
    /// Row container.
    pub row: ElementId,
    /// Name label.
    pub name: ElementId,
    /// Detail row.
    pub detail: ElementId,
    /// Color swatch.
    pub swatch: ElementId,
    /// Plain-text value label.
    pub value: ElementId,
}

/// One realized property-grid row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyGridRealizedRow {
    /// Property name.
    pub name: Box<str>,
    /// Realized element ids.
    pub ids: PropertyGridRowIds,
    /// Badge chip elements owned by this row.
    pub chips: Vec<ElementId>,
}

/// Styling knobs for the property-grid surface.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridStyle {
    /// Row padding.
    pub row_padding: f64,
    /// Gap between row columns.
    pub row_gap: f64,
    /// Name column width.
    pub name_width: f64,
    /// Text font size.
    pub font_size: f64,
    /// Badge font size.
    pub badge_font_size: f64,
    /// Label padding for text values.
    pub label_padding: f64,
    /// Badge text padding.
    pub badge_label_padding: f64,
    /// Badge outer padding.
    pub badge_padding: f64,
    /// Badge corner radius.
    pub badge_corner_radius: f64,
    /// Swatch size.
    pub swatch_size: f64,
    /// Swatch corner radius.
    pub swatch_corner_radius: f64,
    /// Swatch border width.
    pub swatch_border_width: f64,
}

impl Default for PropertyGridStyle {
    fn default() -> Self {
        Self {
            row_padding: 1.0,
            row_gap: 6.0,
            name_width: 84.0,
            font_size: 11.0,
            badge_font_size: 10.0,
            label_padding: 2.0,
            badge_label_padding: 3.0,
            badge_padding: 2.0,
            badge_corner_radius: 6.0,
            swatch_size: 14.0,
            swatch_corner_radius: 3.0,
            swatch_border_width: 1.0,
        }
    }
}

/// Overstory-facing controller for structured property-grid rows.
#[derive(Clone, Debug)]
pub struct PropertyGridController {
    scroll_view: ElementId,
    style: PropertyGridStyle,
    rows: Vec<PropertyGridRealizedRow>,
}

impl PropertyGridController {
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId) -> Self {
        Self {
            scroll_view,
            style: PropertyGridStyle::default(),
            rows: Vec::new(),
        }
    }

    /// Returns the current property-grid style.
    #[must_use]
    pub const fn style(&self) -> &PropertyGridStyle {
        &self.style
    }

    /// Replaces the property-grid style.
    pub fn set_style(&mut self, style: PropertyGridStyle) {
        self.style = style;
    }

    /// Returns the currently realized rows.
    #[must_use]
    pub fn realized_rows(&self) -> &[PropertyGridRealizedRow] {
        &self.rows
    }

    /// Syncs structured rows into Overstory.
    pub fn sync(
        &mut self,
        ui: &mut Ui,
        rows: &[PropertyGridRow],
        swatch_border: Color,
        mut measure_badge_width: impl FnMut(&str) -> f64,
    ) {
        let style = self.style.clone();
        while self.rows.len() < rows.len() {
            self.rows.push(self.append_row(ui, swatch_border));
        }

        for index in 0..self.rows.len() {
            let realized = &mut self.rows[index];
            if let Some(row) = rows.get(index) {
                realized.name = row.name.clone();
                apply_row(&style, ui, &mut measure_badge_width, realized, row);
            } else {
                hide_row(ui, realized);
            }
        }
    }

    fn append_row(&self, ui: &mut Ui, swatch_border: Color) -> PropertyGridRealizedRow {
        let row = ui.append(
            self.scroll_view,
            Row::new()
                .padding(self.style.row_padding)
                .gap(self.style.row_gap)
                .background(Color::TRANSPARENT),
        );

        let name = ui.append(
            row,
            TextBlock::new()
                .label_padding(self.style.label_padding)
                .padding(self.style.row_padding)
                .font_size(self.style.font_size)
                .background(Color::TRANSPARENT),
        );
        ui.set_local(name, ui.properties().width, self.style.name_width);

        let detail = ui.append(
            row,
            Row::new()
                .fill()
                .padding(0.0)
                .gap(self.style.row_gap)
                .background(Color::TRANSPARENT),
        );

        let swatch = ui.append(
            detail,
            Panel::new()
                .width(self.style.swatch_size)
                .height(self.style.swatch_size)
                .corner_radius(self.style.swatch_corner_radius),
        );
        ui.set_local(
            swatch,
            ui.properties().border_width,
            self.style.swatch_border_width,
        );
        ui.set_local(swatch, ui.properties().border_color, swatch_border);

        let value = ui.append(
            detail,
            TextBlock::new()
                .fill()
                .label_padding(self.style.label_padding)
                .padding(self.style.row_padding)
                .font_size(self.style.font_size)
                .background(Color::TRANSPARENT),
        );

        PropertyGridRealizedRow {
            name: Box::from(""),
            ids: PropertyGridRowIds {
                row,
                name,
                detail,
                swatch,
                value,
            },
            chips: Vec::new(),
        }
    }
}

fn apply_row(
    style: &PropertyGridStyle,
    ui: &mut Ui,
    measure_badge_width: &mut impl FnMut(&str) -> f64,
    realized: &mut PropertyGridRealizedRow,
    row: &PropertyGridRow,
) {
    set_text_block_text(ui, realized.ids.name, row.name.clone());
    ui.set_local(realized.ids.row, ui.properties().visible, true);

    match &row.value {
        PropertyValue::Text(value) => {
            set_text_block_text(ui, realized.ids.value, value.clone());
            ui.set_local(realized.ids.swatch, ui.properties().visible, false);
            ui.set_local(realized.ids.value, ui.properties().visible, true);
            hide_extra_chips(ui, realized, 0);
        }
        PropertyValue::Color(color) => {
            set_text_block_text(ui, realized.ids.value, format_color(*color));
            ui.set_local(realized.ids.swatch, ui.properties().visible, true);
            ui.set_local(realized.ids.swatch, ui.properties().background, *color);
            ui.set_local(realized.ids.value, ui.properties().visible, true);
            hide_extra_chips(ui, realized, 0);
        }
        PropertyValue::Badges(badges) => {
            ui.set_local(realized.ids.swatch, ui.properties().visible, false);
            ui.set_local(realized.ids.value, ui.properties().visible, false);

            while realized.chips.len() < badges.len() {
                let chip = ui.append(
                    realized.ids.detail,
                    TextBlock::new()
                        .label_padding(style.badge_label_padding)
                        .padding(style.badge_padding)
                        .font_size(style.badge_font_size)
                        .corner_radius(style.badge_corner_radius),
                );
                realized.chips.push(chip);
            }

            for (chip_id, badge) in realized.chips.iter().zip(badges.iter()) {
                let width = measure_badge_width(badge.label.as_ref())
                    + style.badge_label_padding * 2.0
                    + style.badge_padding * 2.0
                    + 4.0;
                set_text_block_text(ui, *chip_id, badge.label.clone());
                ui.set_local(*chip_id, ui.properties().width, width);
                ui.set_local(*chip_id, ui.properties().background, badge.background);
                ui.set_local(*chip_id, ui.properties().foreground, badge.foreground);
                ui.set_local(*chip_id, ui.properties().visible, true);
            }
            hide_extra_chips(ui, realized, badges.len());
        }
    }
}

fn hide_extra_chips(ui: &mut Ui, realized: &PropertyGridRealizedRow, keep: usize) {
    for chip in realized.chips.iter().skip(keep) {
        ui.set_local(*chip, ui.properties().visible, false);
    }
}

fn hide_row(ui: &mut Ui, realized: &PropertyGridRealizedRow) {
    set_text_block_text(ui, realized.ids.name, "");
    set_text_block_text(ui, realized.ids.value, "");
    ui.set_local(realized.ids.swatch, ui.properties().visible, false);
    ui.set_local(realized.ids.value, ui.properties().visible, false);
    for chip in &realized.chips {
        ui.set_local(*chip, ui.properties().visible, false);
    }
    ui.set_local(realized.ids.row, ui.properties().visible, false);
}

fn set_text_block_text(ui: &mut Ui, id: ElementId, text: impl Into<Box<str>>) {
    ui.widget_mut::<TextBlock>(id)
        .expect("property grid rows use text block children")
        .set_text(text);
}

fn format_color(color: Color) -> String {
    let rgba = color.to_rgba8();
    format!("#{:02x}{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b, rgba.a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use overstory::{ScrollView, default_theme};

    #[test]
    fn sync_realizes_text_color_and_badge_rows() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut grid = PropertyGridController::new(scroll);
        let rows = vec![
            PropertyGridRow::new("name", PropertyValue::Text(Box::from("Deploy"))),
            PropertyGridRow::new(
                "background",
                PropertyValue::Color(Color::from_rgba8(1, 2, 3, 4)),
            ),
            PropertyGridRow::new(
                "state",
                PropertyValue::Badges(vec![PropertyBadge::active(
                    "focused",
                    Color::from_rgba8(9, 9, 9, 255),
                )]),
            ),
        ];

        grid.sync(&mut ui, &rows, Color::from_rgba8(10, 11, 12, 255), |_| 24.0);

        let scene = ui.scene();
        let realized = grid.realized_rows();
        assert_eq!(realized.len(), 3);
        assert!(scene.resolved_element(realized[1].ids.swatch).is_some());
        assert!(scene.resolved_element(realized[2].chips[0]).is_some());
    }
}
