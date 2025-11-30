// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Display list + Vello basics.

use core::cell::Cell;

use kurbo::{Affine, Rect};
use ui_events::pointer::PointerEvent;
use understory_display::{
    ClipId, DisplayList, DisplayListBuilder, DisplayPainter, GroupId, ImageId, PaintId, PathId,
    StrokeId,
};
use understory_display_vello::{ResourceResolver, record_scene};
use understory_examples::display_resources::DisplayResources;
use understory_examples::vello_winit::{VelloDemo, VelloWinitApp};
use vello::Scene;
use vello::peniko::{Brush, Color, ImageBrush};
use winit::event_loop::EventLoop;

/// Minimal scene state shared between drawing, resources, and hit-testing.
struct DemoScene {
    /// Rectangles rendered in the scene and used for hover detection.
    rects: [Rect; 3],
    /// Index of the rectangle currently hovered, if any.
    hover_index: Cell<Option<usize>>,
    /// Geometry and clip resources referenced by the display list.
    resources: DisplayResources,
}

impl ResourceResolver for DemoScene {
    fn path(&self, id: PathId) -> Option<kurbo::BezPath> {
        self.resources.path(id).cloned()
    }

    fn image(&self, _id: ImageId) -> Option<ImageBrush> {
        None
    }

    fn stroke(&self, _id: StrokeId) -> Option<kurbo::Stroke> {
        None
    }

    fn paint(&self, id: PaintId) -> Option<Brush> {
        let idx = (id.0 as usize).wrapping_sub(1);
        let hovered = self.hover_index.get() == Some(idx);
        let base = match idx {
            0 => {
                if hovered {
                    Color::from_rgba8(0x66, 0xcc, 0xff, 0xff)
                } else {
                    Color::from_rgba8(0x33, 0x99, 0xff, 0x99)
                }
            }
            1 => {
                if hovered {
                    Color::from_rgba8(0xff, 0xaa, 0x66, 0xff)
                } else {
                    Color::from_rgba8(0xff, 0x88, 0x33, 0x99)
                }
            }
            2 => {
                if hovered {
                    Color::from_rgba8(0x7f, 0xe0, 0x7f, 0xff)
                } else {
                    Color::from_rgba8(0x4c, 0xc9, 0x4c, 0x99)
                }
            }
            _ => return None,
        };
        Some(Brush::Solid(base))
    }

    fn clip_path(&self, id: ClipId) -> Option<kurbo::BezPath> {
        self.resources.clip(id).cloned()
    }
}

fn build_demo_scene() -> (DisplayList, DemoScene) {
    let rects = [
        Rect::new(30.0, 50.0, 150.0, 150.0),
        Rect::new(130.0, 50.0, 250.0, 150.0),
        Rect::new(230.0, 50.0, 350.0, 150.0),
    ];
    let mut resources = DisplayResources::new();

    let path_ids: [PathId; 3] = [
        resources.add_rect_path(rects[0]),
        resources.add_rect_path(rects[1]),
        resources.add_rect_path(rects[2]),
    ];
    let clip_id = {
        let r = rects[1];
        let clip = Rect::new(r.x0, r.y0, r.x1, (r.y0 + r.y1) * 0.5);
        resources.add_clip_rect(clip)
    };

    let mut b = DisplayListBuilder::new(GroupId(0));
    b.fill_path(0, rects[0], path_ids[0], PaintId(1), None);
    b.push_clip(0, rects[1], clip_id, None);
    b.fill_path(0, rects[1], path_ids[1], PaintId(2), None);
    b.pop_clip(0, rects[1], None);
    b.fill_path(0, rects[2], path_ids[2], PaintId(3), None);
    let scene = DemoScene {
        rects,
        hover_index: Cell::new(None),
        resources,
    };
    (b.finish(), scene)
}

struct Demo {
    display_list: DisplayList,
    demo_scene: DemoScene,
}

impl VelloDemo for Demo {
    fn window_title(&self) -> &'static str {
        "Understory Display + Vello"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (400.0, 300.0)
    }

    fn handle_pointer_event(&mut self, e: PointerEvent) {
        match e {
            PointerEvent::Move(update) => {
                let pos = update.current.logical_position();
                let mut hit = None;
                for (i, r) in self.demo_scene.rects.iter().enumerate() {
                    if pos.x >= r.x0 && pos.x <= r.x1 && pos.y >= r.y0 && pos.y <= r.y1 {
                        hit = Some(i);
                        break;
                    }
                }
                self.demo_scene.hover_index.set(hit);
            }
            PointerEvent::Leave(_) | PointerEvent::Cancel(_) => {
                self.demo_scene.hover_index.set(None);
            }
            _ => {}
        }
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, scale_factor: f64) {
        let xf = Affine::scale(scale_factor);
        record_scene(&self.display_list, &self.demo_scene, scene, xf);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (display_list, demo_scene) = build_demo_scene();
    let demo = Demo {
        display_list,
        demo_scene,
    };
    let mut app = VelloWinitApp::new(demo);

    let event_loop = EventLoop::new()?;
    event_loop
        .run_app(&mut app)
        .expect("Couldn't run event loop");
    Ok(())
}
