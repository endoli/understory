// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Retained element tree primitives.

use alloc::{boxed::Box, vec::Vec};

use understory_property::{DependencyObject, PropertyStore};
use understory_style::{ClassId, IdSet, PseudoClassId, SelectorInputs, StyleCascade, TypeTag};

/// Stable identifier for a retained element.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementId(u32);

impl ElementId {
    pub(crate) fn new(index: usize) -> Self {
        Self(u32::try_from(index).expect("element index exceeds u32 range"))
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Built-in retained element kinds for the first Overstory slice.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ElementKind {
    /// Root viewport element.
    Root,
    /// Decorative/padded container.
    Panel,
    /// Vertical stack container.
    Column,
    /// Interactive push button.
    Button,
    /// Non-interactive spacing element.
    Spacer,
}

/// Type selector for [`ElementKind::Root`].
pub const TYPE_ROOT: TypeTag = TypeTag(1);
/// Type selector for [`ElementKind::Panel`].
pub const TYPE_PANEL: TypeTag = TypeTag(2);
/// Type selector for [`ElementKind::Column`].
pub const TYPE_COLUMN: TypeTag = TypeTag(3);
/// Type selector for [`ElementKind::Button`].
pub const TYPE_BUTTON: TypeTag = TypeTag(4);
/// Type selector for [`ElementKind::Spacer`].
pub const TYPE_SPACER: TypeTag = TypeTag(5);

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
}

/// Hover pseudo selector id.
pub const PSEUDO_HOVER: PseudoClassId = PseudoClassId(1);
/// Pressed pseudo selector id.
pub const PSEUDO_PRESSED: PseudoClassId = PseudoClassId(2);
/// Disabled pseudo selector id.
pub const PSEUDO_DISABLED: PseudoClassId = PseudoClassId(3);

/// One retained element in the Overstory tree.
#[derive(Clone, Debug)]
pub struct Element {
    pub(crate) id: ElementId,
    pub(crate) parent: Option<ElementId>,
    pub(crate) children: Vec<ElementId>,
    pub(crate) kind: ElementKind,
    pub(crate) label: Option<Box<str>>,
    pub(crate) store: PropertyStore<ElementId>,
    pub(crate) classes: IdSet<ClassId>,
    pub(crate) style: Option<StyleCascade>,
    pub(crate) pseudos: PseudoState,
}

impl Element {
    pub(crate) fn new(id: ElementId, parent: Option<ElementId>, kind: ElementKind) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            kind,
            label: None,
            store: PropertyStore::new(id),
            classes: IdSet::default(),
            style: None,
            pseudos: PseudoState::default(),
        }
    }

    /// Returns the element id.
    #[must_use]
    pub const fn id(&self) -> ElementId {
        self.id
    }

    /// Returns the element kind.
    #[must_use]
    pub const fn kind(&self) -> ElementKind {
        self.kind
    }

    /// Returns the optional label text.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub(crate) fn selector_inputs<'a>(
        &'a self,
        pseudos: &'a [PseudoClassId],
    ) -> SelectorInputs<'a> {
        SelectorInputs::new(Some(self.kind.type_tag()), self.classes.as_slice(), pseudos)
    }
}

impl ElementKind {
    /// Returns the built-in style type tag.
    #[must_use]
    pub const fn type_tag(self) -> TypeTag {
        match self {
            Self::Root => TYPE_ROOT,
            Self::Panel => TYPE_PANEL,
            Self::Column => TYPE_COLUMN,
            Self::Button => TYPE_BUTTON,
            Self::Spacer => TYPE_SPACER,
        }
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
