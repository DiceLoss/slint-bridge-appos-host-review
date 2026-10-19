//! Bounded CPU/VE image decoding through the public application API.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(bytes: usize) {
    let current = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(current, Ordering::Relaxed);
}
// SAFETY: all allocation operations delegate to System with their original layout.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            allocated(layout.size());
        }
        result
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc_zeroed(layout) };
        if !result.is_null() {
            allocated(layout.size());
        }
        result
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(pointer, layout);
        }
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            if size >= layout.size() {
                allocated(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;

fn hardware(data: &[u8]) -> slint::Image {
    match slint_bridge::try_decode_png(data) {
        Ok(Some(image)) => image,
        Ok(None) => panic!("hardware PNG path was not admitted"),
        Err(error) => {
            eprintln!("F1_PNG_QUARANTINED: {error}; retain the process for recovery");
            loop {
                std::thread::park();
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("verify");
    if !["verify", "software", "hardware"].contains(&mode) {
        return Err(
            "usage: image-decode [verify|software|hardware] [case|all] [rounds<=25]".into(),
        );
    }
    let selected = args.get(2).map(String::as_str).unwrap_or("all");
    let rounds: usize = args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(20);
    if rounds == 0 || rounds > 25 {
        return Err("rounds must be 1..=25".into());
    }
    let cases: &[(&str, &[u8])] = &[
        (
            "background",
            include_bytes!(concat!(env!("OUT_DIR"), "/coastal-background.png")),
        ),
        (
            "scaling",
            include_bytes!("../../ui/assets/scaling-reference.png"),
        ),
        (
            "row-blue",
            include_bytes!("../../ui/assets/rich-row-blue.png"),
        ),
        (
            "row-amber",
            include_bytes!("../../ui/assets/rich-row-amber.png"),
        ),
        (
            "card-blue",
            include_bytes!("../../ui/assets/rich-card-blue.png"),
        ),
        (
            "card-amber",
            include_bytes!("../../ui/assets/rich-card-amber.png"),
        ),
        (
            "rgb-filters",
            include_bytes!("../../fixtures/png/rgb-filters.png"),
        ),
        (
            "rgba-filters",
            include_bytes!("../../fixtures/png/rgba-filters.png"),
        ),
    ];
    if selected != "all" && !cases.iter().any(|(name, _)| *name == selected) {
        return Err("unknown PNG case".into());
    }
    for &(name, data) in cases
        .iter()
        .filter(|(name, _)| selected == "all" || selected == *name)
    {
        if mode == "verify" {
            let expected = slint::Image::load_from_data(data, Some("png"))?
                .to_rgba8_premultiplied()
                .unwrap();
            let actual = hardware(data).to_rgba8_premultiplied().unwrap();
            assert_eq!(actual.size(), expected.size());
            assert_eq!(actual.as_bytes(), expected.as_bytes(), "PNG {name}");
            println!(
                "F1_PNG_VERIFIED name={name} pixels={} bytes={}",
                actual.as_slice().len(),
                actual.as_bytes().len()
            );
            continue;
        }
        for iteration in 0..rounds + 3 {
            let before = LIVE.load(Ordering::Relaxed);
            PEAK.store(before, Ordering::Relaxed);
            let start = Instant::now();
            let image = if mode == "hardware" {
                hardware(data)
            } else {
                slint::Image::load_from_data(data, Some("png"))?
            };
            let elapsed = start.elapsed().as_micros();
            let peak = PEAK.load(Ordering::Relaxed).saturating_sub(before);
            let retained = LIVE.load(Ordering::Relaxed).saturating_sub(before);
            if iteration >= 3 {
                println!(
                    "F1_PNG_SAMPLE mode={mode} name={name} round={} us={elapsed} heap_peak_extra={peak} heap_retained_extra={retained}",
                    iteration - 3
                );
            }
            std::hint::black_box(&image);
            drop(image);
        }
    }
    println!("F1_PNG_PROBE_DONE mode={mode}");
    for (path, fields) in [
        ("/proc/self/status", &["VmRSS:", "VmHWM:"][..]),
        ("/proc/meminfo", &["MemAvailable:", "CmaFree:"][..]),
    ] {
        for line in std::fs::read_to_string(path)?
            .lines()
            .filter(|line| fields.iter().any(|field| line.starts_with(field)))
        {
            println!("{line}");
        }
    }
    Ok(())
}
