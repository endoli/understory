// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Retained element tree primitives.

use alloc::{boxed::Box, vec::Vec};

use understory_property::{DependencyObject, PropertyStore};

use crate::WidgetHandle;
use understory_style::{ClassId, IdSet, PseudoClassId, SelectorInputs, StyleCascade, TypeTag};

/// Stable identifier for a retained element.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementId(u32);

impl ElementId {
    pub(crate) fn new(index: usize) -> Self {
        Self(u32::try_from(index).expect("element index exceeds u32 range"))
    }

    /// Returns the numeric index of this element.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Type tag for root viewport elements.
pub const TYPE_ROOT: TypeTag = TypeTag(1);
/// Type tag for decorative/padded containers.
pub const TYPE_PANEL: TypeTag = TypeTag(2);
/// Type tag for horizontal stack containers.
pub const TYPE_ROW: TypeTag = TypeTag(3);
/// Type tag for vertical stack containers.
pub const TYPE_COLUMN: TypeTag = TypeTag(4);
/// Type tag for interactive push buttons.
pub const TYPE_BUTTON: TypeTag = TypeTag(5);
/// Type tag for non-interactive spacing elements.
pub const TYPE_SPACER: TypeTag = TypeTag(6);
/// Type tag for scrollable vertical containers.
pub const TYPE_SCROLL_VIEW: TypeTag = TypeTag(7);
/// Type tag for multiline wrapped text blocks.
pub const TYPE_TEXT_BLOCK: TypeTag = TypeTag(8);
/// Type tag for single-line text inputs.
pub const TYPE_TEXT_INPUT: TypeTag = TypeTag(9);
/// Type tag for tooltip overlays.
pub const TYPE_TOOLTIP: TypeTag = TypeTag(10);
/// Type tag for draggable pane splitters.
pub const TYPE_SPLITTER: TypeTag = TypeTag(11);
/// Type tag for decorative divider rules.
pub const TYPE_DIVIDER: TypeTag = TypeTag(12);
/// Type tag for animated loading indicators.
pub const TYPE_SPINNER: TypeTag = TypeTag(13);

/// Small class vocabulary for common button styling.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonClass {
    /// High-emphasis action.
    Primary,
}

impl ButtonClass {
    /// Returns the matching style class identifier.
    #[must_use]
    pub const fn class_id(self) -> ClassId {
        match self {
            Self::Primary => ClassId(1),
        }
    }
}

/// Small class vocabulary for container/layout styling.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayoutClass {
    /// Sidebar-like container variant.
    Sidebar,
}

impl LayoutClass {
    /// Returns the matching style class identifier.
    #[must_use]
    pub const fn class_id(self) -> ClassId {
        match self {
            Self::Sidebar => ClassId(10),
        }
    }
}

/// Class vocabulary for message styling.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MessageClass {
    /// User-authored message.
    User,
}

impl MessageClass {
    /// Returns the matching style class identifier.
    #[must_use]
    pub const fn class_id(self) -> ClassId {
        match self {
            Self::User => ClassId(20),
        }
    }
}

/// Dynamic pseudo-state used during runtime interaction.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PseudoState {
    /// Pointer currently hovers this element or one of its descendants in the
    /// current Overstory first-slice runtime.
    pub hovered: bool,
    /// The element is the active press target.
    pub pressed: bool,
    /// The element is disabled.
    pub disabled: bool,
    /// The element has keyboard focus.
    pub focused: bool,
}

/// Hover pseudo selector id.
pub const PSEUDO_HOVER: PseudoClassId = PseudoClassId(1);
/// Pressed pseudo selector id.
pub const PSEUDO_PRESSED: PseudoClassId = PseudoClassId(2);
/// Disabled pseudo selector id.
pub const PSEUDO_DISABLED: PseudoClassId = PseudoClassId(3);
/// Focused pseudo selector id.
pub const PSEUDO_FOCUSED: PseudoClassId = PseudoClassId(4);

/// One retained element in the Overstory tree.
#[derive(Clone)]
pub struct Element {
    pub(crate) id: ElementId,
    pub(crate) parent: Option<ElementId>,
    pub(crate) children: Vec<ElementId>,
    /// Style type tag for selector matching.
    pub(crate) type_tag: TypeTag,
    /// Whether this element is a container that lays out children.
    pub(crate) is_container: bool,
    /// Whether this container lays out children horizontally (Row).
    pub(crate) horizontal: bool,
    /// Whether this element is the root viewport element.
    pub(crate) is_root: bool,
    /// Short human-readable name for debugging and inspection.
    ///
    /// This is not the authoritative text content for text-bearing widgets.
    /// Widgets that own text should store it themselves and expose it through
    /// their widget implementation.
    pub(crate) display_name: Option<Box<str>>,
    pub(crate) store: PropertyStore<ElementId>,
    pub(crate) classes: IdSet<ClassId>,
    pub(crate) style: Option<StyleCascade>,
    pub(crate) pseudos: PseudoState,
    /// Optional widget handle for kind-specific behavior.
    pub(crate) widget: Option<WidgetHandle>,
}

impl core::fmt::Debug for Element {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Element")
            .field("id", &self.id)
            .field("type_tag", &self.type_tag)
            .field("children", &self.children.len())
            .finish_non_exhaustive()
    }
}

impl Element {
    pub(crate) fn new(id: ElementId, parent: Option<ElementId>, type_tag: TypeTag) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            type_tag,
            is_container: false,
            horizontal: false,
            is_root: false,
            display_name: None,
            store: PropertyStore::new(id),
            classes: IdSet::default(),
            style: None,
            pseudos: PseudoState::default(),
            widget: None,
        }
    }

    /// Returns the element id.
    #[must_use]
    pub const fn id(&self) -> ElementId {
        self.id
    }

    /// Returns the element's style type tag.
    #[must_use]
    pub const fn type_tag(&self) -> TypeTag {
        self.type_tag
    }

    /// Returns the parent element id, if any.
    #[must_use]
    pub fn parent(&self) -> Option<ElementId> {
        self.parent
    }

    /// Returns the direct children of this element.
    #[must_use]
    pub fn children(&self) -> &[ElementId] {
        &self.children
    }

    /// Returns the optional human-readable display name.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    /// Returns the optional human-readable display name.
    ///
    /// This is retained as a compatibility alias while callers move toward
    /// [`Self::display_name`].
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.display_name()
    }

    pub(crate) fn selector_inputs<'a>(
        &'a self,
        pseudos: &'a [PseudoClassId],
    ) -> SelectorInputs<'a> {
        SelectorInputs::new(Some(self.type_tag), self.classes.as_slice(), pseudos)
    }
}

impl DependencyObject<ElementId> for Element {
    fn property_store(&self) -> &PropertyStore<ElementId> {
        &self.store
    }

    fn property_store_mut(&mut self) -> &mut PropertyStore<ElementId> {
        &mut self.store
    }

    fn key(&self) -> ElementId {
        self.id
    }

    fn parent_key(&self) -> Option<ElementId> {
        self.parent
    }
}
