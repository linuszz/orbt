pub mod frame;

pub use frame::{Frame, Size, Rect, Cursor, CursorKind, InteractiveRegion, InteractiveAction, ClientId};

use crate::types::SurfaceId;
use orbt_protocol::Cell;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SurfaceGrid {
    surface_id: SurfaceId,
    width: u16,
    height: u16,
    cells: Vec<Cell>,
    dirty_region: Option<frame::Rect>,
}

impl SurfaceGrid {
    pub fn new(surface_id: SurfaceId, width: u16, height: u16) -> Self {
        let cells = vec![Cell::default(); (width * height) as usize];
        Self {
            surface_id,
            width,
            height,
            cells,
            dirty_region: Some(frame::Rect {
                x: 0,
                y: 0,
                w: width,
                h: height,
            }),
        }
    }

    pub fn apply_frame(&mut self, frame: &frame::Frame) {
        match frame.dirty {
            None => {
                self.width = frame.size.w;
                self.height = frame.size.h;
                self.cells = frame.buffer.clone();
                self.dirty_region = Some(frame::Rect {
                    x: 0,
                    y: 0,
                    w: frame.size.w,
                    h: frame.size.h,
                });
            }
            Some(rect) => {
                for dy in 0..rect.h {
                    for dx in 0..rect.w {
                        let src_idx = (dy * rect.w + dx) as usize;
                        let dst_x = rect.x + dx;
                        let dst_y = rect.y + dy;
                        if dst_x < self.width && dst_y < self.height {
                            let dst_idx = (dst_y * self.width + dst_x) as usize;
                            if src_idx < frame.buffer.len() && dst_idx < self.cells.len() {
                                self.cells[dst_idx] = frame.buffer[src_idx];
                            }
                        }
                    }
                }
                self.dirty_region = Some(rect);
            }
        }
    }

    pub fn take_dirty_region(&mut self) -> Option<frame::Rect> {
        self.dirty_region.take()
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }
}

pub struct SurfaceGridManager {
    grids: HashMap<SurfaceId, SurfaceGrid>,
}

impl SurfaceGridManager {
    pub fn new() -> Self {
        Self {
            grids: HashMap::new(),
        }
    }

    pub fn ensure_grid(&mut self, surface_id: SurfaceId, width: u16, height: u16) {
        self.grids
            .entry(surface_id)
            .or_insert_with(|| SurfaceGrid::new(surface_id, width, height));
    }

    pub fn apply_frame(&mut self, surface_id: SurfaceId, frame: &frame::Frame) {
        self.ensure_grid(surface_id, frame.size.w, frame.size.h);
        if let Some(grid) = self.grids.get_mut(&surface_id) {
            grid.apply_frame(frame);
        }
    }

    pub fn get_grid(&self, surface_id: &SurfaceId) -> Option<&SurfaceGrid> {
        self.grids.get(surface_id)
    }

    pub fn get_grid_mut(&mut self, surface_id: &SurfaceId) -> Option<&mut SurfaceGrid> {
        self.grids.get_mut(surface_id)
    }

    pub fn remove_grid(&mut self, surface_id: &SurfaceId) -> Option<SurfaceGrid> {
        self.grids.remove(surface_id)
    }
}

impl Default for SurfaceGridManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_frame_replaces_grid() {
        let mut grid = SurfaceGrid::new(SurfaceId(1), 4, 4);
        let frame = frame::Frame {
            size: frame::Size { w: 2, h: 2 },
            dirty: None,
            buffer: vec![
                Cell::default(),
                Cell::default(),
                Cell::default(),
                Cell::default(),
            ],
            cursor: None,
            interactives: vec![],
            target_client: None,
        };
        grid.apply_frame(&frame);
        assert_eq!(grid.size(), (2, 2));
        assert_eq!(grid.cells().len(), 4);
    }

    #[test]
    fn partial_frame_updates_region() {
        let mut grid = SurfaceGrid::new(SurfaceId(1), 4, 4);
        let frame = frame::Frame {
            size: frame::Size { w: 4, h: 4 },
            dirty: Some(frame::Rect { x: 1, y: 1, w: 2, h: 2 }),
            buffer: vec![
                Cell::default(),
                Cell::default(),
                Cell::default(),
                Cell::default(),
            ],
            cursor: None,
            interactives: vec![],
            target_client: None,
        };
        grid.apply_frame(&frame);
        assert_eq!(grid.size(), (4, 4));
    }
}
