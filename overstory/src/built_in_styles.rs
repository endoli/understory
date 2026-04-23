// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Built-in element/widget style cascades expressed in semantic theme tokens.
//!
//! This module is the default widget-policy layer for Overstory:
//!
//! - [`crate::ThemeKeys`] provides semantic tokens.
//! - these built-in cascades decide which tokens apply to built-in controls and
//!   interaction states.
//! - host-provided element styles are composed *after* these defaults in scene
//!   resolution.
//!
//! This keeps widget styling policy in the stylesheet system instead of
//! scattering it across widget-specific theme-key hooks.

use understory_style::{
    IdSet, Selector, StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin,
    StyleSheetBuilder,
};

use crate::{
    BuiltInProperties, ButtonClass, Element, LayoutClass, MessageClass, PSEUDO_HOVER,
    PSEUDO_PRESSED, TYPE_BUTTON, TYPE_PANEL, TYPE_ROOT, TYPE_SCROLL_VIEW, TYPE_SPLITTER,
    TYPE_TEXT_BLOCK, TYPE_TEXT_INPUT, TYPE_TOOLTIP, ThemeKeys,
};

#[derive(Clone, Debug)]
pub(crate) struct BuiltInStyles {
    button: StyleCascade,
    panel: StyleCascade,
    root: StyleCascade,
    scroll_view: StyleCascade,
    splitter: StyleCascade,
    text_block: StyleCascade,
    text_input: StyleCascade,
    tooltip: StyleCascade,
}

impl BuiltInStyles {
    pub(crate) fn new(props: &BuiltInProperties) -> Self {
        Self {
            button: button_styles(props),
            panel: panel_styles(props),
            root: root_styles(props),
            scroll_view: scroll_view_styles(props),
            splitter: splitter_styles(props),
            text_block: text_block_styles(props),
            text_input: text_input_styles(props),
            tooltip: tooltip_styles(props),
        }
    }

    pub(crate) fn for_element(&self, element: &Element) -> Option<&StyleCascade> {
        match element.type_tag() {
            TYPE_BUTTON => Some(&self.button),
            TYPE_PANEL => Some(&self.panel),
            TYPE_ROOT => Some(&self.root),
            TYPE_SCROLL_VIEW => Some(&self.scroll_view),
            TYPE_SPLITTER => Some(&self.splitter),
            TYPE_TEXT_BLOCK => Some(&self.text_block),
            TYPE_TEXT_INPUT => Some(&self.text_input),
            TYPE_TOOLTIP => Some(&self.tooltip),
            _ => None,
        }
    }
}

fn root_styles(props: &BuiltInProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set_resource(props.background, ThemeKeys::APP_BACKGROUND)
                .build(),
        )
        .build()
}

fn panel_styles(props: &BuiltInProperties) -> StyleCascade {
    let base = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::SURFACE_BACKGROUND)
        .build();
    let sidebar = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::SURFACE_MUTED_BACKGROUND)
        .build();
    let selector_sidebar = Selector {
        type_tag: Some(TYPE_PANEL),
        required_classes: IdSet::from_ids([LayoutClass::Sidebar.class_id()]),
        required_pseudos: IdSet::default(),
    };

    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_sheet(
            StyleOrigin::Sheet,
            StyleSheetBuilder::new()
                .rule(selector_sidebar, sidebar)
                .build(),
        )
        .build()
}

fn button_styles(props: &BuiltInProperties) -> StyleCascade {
    let base = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::CONTROL_BACKGROUND)
        .set_resource(props.height, ThemeKeys::BUTTON_HEIGHT)
        .build();
    let hover = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::CONTROL_BACKGROUND_EMPHASIZED)
        .build();
    let pressed = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::CONTROL_BACKGROUND_STRONG)
        .build();
    let primary = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::ACCENT_BACKGROUND)
        .set_resource(props.foreground, ThemeKeys::ACCENT_FOREGROUND)
        .build();
    let primary_hover = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::ACCENT_BACKGROUND_EMPHASIZED)
        .build();
    let primary_pressed = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::ACCENT_BACKGROUND_STRONG)
        .build();

    let selector_hover = Selector {
        type_tag: Some(TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([PSEUDO_HOVER]),
    };
    let selector_pressed = Selector {
        type_tag: Some(TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([PSEUDO_PRESSED]),
    };
    let selector_primary = Selector {
        type_tag: Some(TYPE_BUTTON),
        required_classes: IdSet::from_ids([ButtonClass::Primary.class_id()]),
        required_pseudos: IdSet::default(),
    };
    let selector_primary_hover = Selector {
        type_tag: Some(TYPE_BUTTON),
        required_classes: IdSet::from_ids([ButtonClass::Primary.class_id()]),
        required_pseudos: IdSet::from_ids([PSEUDO_HOVER]),
    };
    let selector_primary_pressed = Selector {
        type_tag: Some(TYPE_BUTTON),
        required_classes: IdSet::from_ids([ButtonClass::Primary.class_id()]),
        required_pseudos: IdSet::from_ids([PSEUDO_PRESSED]),
    };

    let sheet = StyleSheetBuilder::new()
        .rule(selector_hover, hover)
        .rule(selector_pressed, pressed)
        .rule(selector_primary, primary)
        .rule(selector_primary_hover, primary_hover)
        .rule(selector_primary_pressed, primary_pressed)
        .build();

    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_sheet(StyleOrigin::Sheet, sheet)
        .build()
}

fn scroll_view_styles(props: &BuiltInProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set_resource(props.background, ThemeKeys::SURFACE_BACKGROUND)
                .build(),
        )
        .build()
}

fn splitter_styles(props: &BuiltInProperties) -> StyleCascade {
    let hover = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::DIVIDER_BACKGROUND_EMPHASIZED)
        .build();
    let pressed = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::DIVIDER_BACKGROUND_STRONG)
        .build();

    let selector_hover = Selector {
        type_tag: Some(TYPE_SPLITTER),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([PSEUDO_HOVER]),
    };
    let selector_pressed = Selector {
        type_tag: Some(TYPE_SPLITTER),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([PSEUDO_PRESSED]),
    };

    let sheet = StyleSheetBuilder::new()
        .rule(selector_hover, hover)
        .rule(selector_pressed, pressed)
        .build();

    StyleCascadeBuilder::new()
        .push_sheet(StyleOrigin::Sheet, sheet)
        .build()
}

fn text_block_styles(props: &BuiltInProperties) -> StyleCascade {
    let user_message = StyleBuilder::new()
        .set_resource(props.background, ThemeKeys::CONTROL_BACKGROUND)
        .build();
    let selector_user = Selector {
        type_tag: Some(TYPE_TEXT_BLOCK),
        required_classes: IdSet::from_ids([MessageClass::User.class_id()]),
        required_pseudos: IdSet::default(),
    };

    StyleCascadeBuilder::new()
        .push_sheet(
            StyleOrigin::Sheet,
            StyleSheetBuilder::new()
                .rule(selector_user, user_message)
                .build(),
        )
        .build()
}

fn text_input_styles(props: &BuiltInProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set_resource(props.background, ThemeKeys::SURFACE_BACKGROUND)
                .set_resource(props.height, ThemeKeys::BUTTON_HEIGHT)
                .build(),
        )
        .build()
}

fn tooltip_styles(props: &BuiltInProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set_resource(props.background, ThemeKeys::CONTROL_BACKGROUND)
                .build(),
        )
        .build()
}
