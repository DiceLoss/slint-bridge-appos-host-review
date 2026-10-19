//! SVG parsing and physical-size rasterization owned by the installed platform.
use resvg::{tiny_skia, usvg};
use slint::{
    PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    platform::{SvgError, SvgImage, SvgOptions},
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::{Rc, Weak},
};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 32;

struct Entry {
    source: Weak<usvg::Tree>,
    size: Option<PhysicalSize>,
    pixels: SharedPixelBuffer<Rgba8Pixel>,
}

#[derive(Default)]
pub(crate) struct SvgImages {
    cache: Rc<RefCell<VecDeque<Entry>>>,
}

impl SvgImages {
    pub(crate) fn load(
        &self,
        data: &[u8],
        options: &SvgOptions<'_>,
    ) -> Result<Box<dyn SvgImage>, SvgError> {
        Ok(Box::new(Image {
            tree: Rc::new(usvg::Tree::from_data(data, options)?),
            cache: self.cache.clone(),
        }))
    }
}

struct Image {
    tree: Rc<usvg::Tree>,
    cache: Rc<RefCell<VecDeque<Entry>>>,
}

impl SvgImage for Image {
    fn size(&self) -> PhysicalSize {
        let size = self.tree.size().to_int_size();
        PhysicalSize::new(size.width(), size.height())
    }

    fn render(
        &self,
        size: Option<PhysicalSize>,
    ) -> Result<SharedPixelBuffer<Rgba8Pixel>, SvgError> {
        let source = Rc::downgrade(&self.tree);
        {
            let mut cache = self.cache.borrow_mut();
            if let Some(index) = cache
                .iter()
                .position(|entry| entry.source.ptr_eq(&source) && entry.size == size)
            {
                let entry = cache.remove(index).unwrap();
                let pixels = entry.pixels.clone();
                cache.push_back(entry);
                return Ok(pixels);
            }
        }
        let intrinsic = self.tree.size().to_int_size();
        let (target, transform) = match size {
            Some(size) => {
                let requested = tiny_skia::IntSize::from_wh(size.width, size.height)
                    .ok_or(SvgError::InvalidSize)?;
                let target = intrinsic.scale_to(requested);
                let transform = tiny_skia::Transform::from_scale(
                    target.width() as f32 / self.tree.size().width(),
                    target.height() as f32 / self.tree.size().height(),
                );
                (target, transform)
            }
            None => (intrinsic, tiny_skia::Transform::default()),
        };
        let mut pixels = SharedPixelBuffer::new(target.width(), target.height());
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(
            pixels.make_mut_bytes(),
            target.width(),
            target.height(),
        )
        .ok_or(SvgError::InvalidSize)?;
        resvg::render(&self.tree, transform, &mut pixmap);
        let bytes = pixels.as_bytes().len();
        let mut cache = self.cache.borrow_mut();
        cache.retain(|entry| !entry.source.ptr_eq(&source) && entry.source.strong_count() > 0);
        if bytes <= MAX_BYTES {
            while cache.len() >= MAX_ENTRIES
                || cache
                    .iter()
                    .map(|entry| entry.pixels.as_bytes().len())
                    .sum::<usize>()
                    + bytes
                    > MAX_BYTES
            {
                cache.pop_front();
            }
            cache.push_back(Entry {
                source,
                size,
                pixels: pixels.clone(),
            });
        }
        Ok(pixels)
    }
}

#[cfg(test)]
#[path = "tests/svg_cache.rs"]
mod tests;
