//! Final DE2 output alpha on the inspected NX1 firmware.
//!
//! The kernel clears BLEND_OUTPUT_CTL.premultiply_en when enabling the manager.
//! A transparent, non-premultiplied result then reaches an RGB-only LCD without
//! its final alpha multiplication. Keep layer pixels/alpha unchanged and apply
//! that multiplication after the hardware has composited all layers.
use std::{
    fs::{File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::atomic::{Ordering, fence},
};

const PHYSICAL_BASE: libc::off_t = 0x0110_1000;
const MAPPING_BYTES: usize = 0x4000;
const OUTPUT_SIZE: usize = 0x8c;
const OUTPUT_CONTROL: usize = 0xfc;
const UI_ATTRIBUTE: usize = 0x3000;
const UI_SIZE: usize = 0x3004;
const PANEL_SIZE: u32 = (599 << 16) | 1023;

pub(super) struct OutputAlpha {
    registers: *mut u32,
    original: u32,
    restore: bool,
}

// SAFETY: the mapping is owned, and all access is serialized by the NX1 session
// mutex. The presentation worker is joined before the final session is dropped.
unsafe impl Send for OutputAlpha {}

impl OutputAlpha {
    pub(super) fn open() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_SYNC)
            .open("/dev/mem")?;
        Self::from_file(&file, PHYSICAL_BASE)
    }

    fn from_file(file: &File, offset: libc::off_t) -> io::Result<Self> {
        // SAFETY: the inspected DE register range is page-aligned and mapped
        // without executable permissions; tests use a same-sized private file.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                MAPPING_BYTES,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                offset,
            )
        };
        if mapping == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let mut output = Self {
            registers: mapping.cast(),
            original: 0,
            restore: false,
        };
        output.check_layout()?;
        output.original = output.read(OUTPUT_CONTROL);
        output.write_output(output.original | 1);
        output.restore = true;
        output.check()?;
        Ok(output)
    }

    fn read(&self, offset: usize) -> u32 {
        debug_assert!(offset.is_multiple_of(4) && offset < MAPPING_BYTES);
        // SAFETY: all call sites use aligned offsets inside this live mapping.
        unsafe { self.registers.add(offset / 4).read_volatile() }
    }

    fn write_output(&self, value: u32) {
        // SAFETY: only the validated output-control word is writable by this
        // module. No layer, framebuffer, DMA, interrupt or timing register is set.
        unsafe { self.registers.add(OUTPUT_CONTROL / 4).write_volatile(value) };
        fence(Ordering::SeqCst);
    }

    fn check_layout(&self) -> io::Result<()> {
        if self.read(OUTPUT_SIZE) != PANEL_SIZE
            || self.read(UI_SIZE) != PANEL_SIZE
            || self.read(UI_ATTRIBUTE) != 0xff00_0001
            || self.read(OUTPUT_CONTROL) & !1 != 0
        {
            return Err(io::Error::other(
                "NX1 compositor layout or alpha mode changed",
            ));
        }
        Ok(())
    }

    pub(super) fn check(&self) -> io::Result<()> {
        self.check_layout()?;
        if self.read(OUTPUT_CONTROL) != 1 {
            return Err(io::Error::other("NX1 final output alpha is disabled"));
        }
        Ok(())
    }
}

impl Drop for OutputAlpha {
    fn drop(&mut self) {
        // Do not overwrite a changed display configuration. The owning session
        // still holds the exclusive display lock while this restoration runs.
        if self.restore && self.check().is_ok() {
            self.write_output(self.original);
            if self.read(OUTPUT_CONTROL) != self.original {
                eprintln!("slint-bridge: NX1 output alpha restoration failed");
            }
        }
        // SAFETY: this is the same mapping and length returned by mmap above.
        unsafe { libc::munmap(self.registers.cast(), MAPPING_BYTES) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::{fd::FromRawFd, unix::fs::FileExt};

    fn registers(output: u32) -> File {
        // SAFETY: valid NUL-terminated name; ownership of the new fd is taken.
        let fd = unsafe { libc::syscall(libc::SYS_memfd_create, c"nx1-alpha".as_ptr(), 0) };
        assert!(fd >= 0);
        let file = unsafe { File::from_raw_fd(fd as _) };
        file.set_len(MAPPING_BYTES as u64).unwrap();
        for (offset, value) in [
            (0x00, 0x301),       // Two active mixer inputs.
            (0x80, 0x21),        // Blue underlay then the UI channel.
            (0x88, 0xff12_3456), // Existing background must remain untouched.
            (0x2000, 0x11),
            (0x20c0, 0xff20_4080),
            (OUTPUT_SIZE, PANEL_SIZE),
            (UI_SIZE, PANEL_SIZE),
            (UI_ATTRIBUTE, 0xff00_0001),
            (OUTPUT_CONTROL, output),
        ] {
            file.write_all_at(&value.to_ne_bytes(), offset as u64)
                .unwrap();
        }
        file
    }

    fn bytes(file: &File) -> Vec<u8> {
        let mut data = vec![0; MAPPING_BYTES];
        file.read_exact_at(&mut data, 0).unwrap();
        data
    }

    #[test]
    fn transparent_output_enables_only_final_multiplication_and_restores_it() {
        for previous in [0, 1] {
            let file = registers(previous);
            let before = bytes(&file);
            let output = OutputAlpha::from_file(&file, 0).unwrap();
            output.check().unwrap();
            let mut expected = before.clone();
            expected[OUTPUT_CONTROL..OUTPUT_CONTROL + 4].copy_from_slice(&1u32.to_ne_bytes());
            assert_eq!(bytes(&file), expected);
            drop(output);
            assert_eq!(bytes(&file), before);
        }
    }

    #[test]
    fn incompatible_layout_is_rejected_without_writing_registers() {
        for (offset, value) in [
            (OUTPUT_SIZE, 0u32),
            (UI_SIZE, 0),
            (UI_ATTRIBUTE, 0xff00_0003), // Global instead of pixel alpha.
            (UI_ATTRIBUTE, 0xff02_0001), // Premultiplied instead of straight input.
            (OUTPUT_CONTROL, 2),         // Interlaced output.
        ] {
            let file = registers(0);
            file.write_all_at(&value.to_ne_bytes(), offset as u64)
                .unwrap();
            let before = bytes(&file);
            assert!(OutputAlpha::from_file(&file, 0).is_err());
            assert_eq!(bytes(&file), before);
        }
    }

    #[test]
    fn changed_output_is_detected_and_not_overwritten_on_drop() {
        for (offset, value) in [
            (OUTPUT_CONTROL, 0u32),
            (OUTPUT_SIZE, 0),
            (UI_ATTRIBUTE, 0xff00_0003),
        ] {
            let file = registers(0);
            let output = OutputAlpha::from_file(&file, 0).unwrap();
            file.write_all_at(&value.to_ne_bytes(), offset as u64)
                .unwrap();
            assert!(output.check().is_err());
            let changed = bytes(&file);
            drop(output);
            assert_eq!(bytes(&file), changed);
        }
    }
}
