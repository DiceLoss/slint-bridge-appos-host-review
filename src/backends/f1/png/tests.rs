use super::*;

fn chunk(kind: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut output = (bytes.len() as u32).to_be_bytes().to_vec();
    output.extend_from_slice(kind);
    output.extend_from_slice(bytes);
    output.extend_from_slice(&crc32fast::hash(&output[4..]).to_be_bytes());
    output
}

fn envelope(width: u32, height: u32, color: u8, depth: u8, interlace: u8) -> Vec<u8> {
    let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = width.to_be_bytes().to_vec();
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[depth, color, 0, 0, interlace]);
    output.extend(chunk(b"IHDR", &header));
    output
}

fn finish(mut output: Vec<u8>) -> Vec<u8> {
    output.extend(chunk(b"IDAT", &[0x78, 0x9c, 0, 0, 0, 0, 0]));
    output.extend(chunk(b"IEND", &[]));
    output
}

#[test]
fn accepts_real_asset_and_idat_header_split_without_extra_encoded_copy() {
    let png = Png::parse(include_bytes!(
        "../../../../test-app/ui/assets/scaling-reference.png"
    ))
    .unwrap();
    assert_eq!(
        (png.width, png.height, png.color, png.stride),
        (256, 160, 6, 1024)
    );
    assert!(png.bitstream_len() >= png.idat_len + 64);
    assert!(png.bitstream_len().is_multiple_of(4096));
    let mut data = envelope(17, 13, 6, 8, 0);
    data.extend(chunk(b"IDAT", &[]));
    data.extend(chunk(b"IDAT", &[0x78]));
    data.extend(chunk(b"IDAT", &[0x9c, 0, 0, 0, 0, 0]));
    data.extend(chunk(b"IEND", &[]));
    let png = Png::parse(&data).unwrap();
    let mut copied = [0xa5; 11];
    png.copy_idat(&mut copied[..7]);
    assert_eq!(&copied[..7], &[0x78, 0x9c, 0, 0, 0, 0, 0]);
    assert_eq!(&copied[7..], &[0xa5; 4]);
    assert_eq!(png.output_len(), 72 * 13);
}

#[test]
fn rejects_unsupported_or_malformed_png_before_hardware() {
    for (w, h, color, depth, interlace) in [
        (0, 10, 6, 8, 0),
        (10, 0, 6, 8, 0),
        (1024, 601, 6, 8, 0),
        (4097, 1, 6, 8, 0),
        (1, u32::MAX, 6, 8, 0),
        (10, 10, 0, 8, 0),
        (10, 10, 3, 8, 0),
        (10, 10, 6, 16, 0),
        (10, 10, 6, 8, 1),
    ] {
        assert!(Png::parse(&finish(envelope(w, h, color, depth, interlace))).is_none());
    }
    let valid = finish(envelope(17, 13, 6, 8, 0));
    for length in 0..valid.len() {
        assert!(Png::parse(&valid[..length]).is_none());
    }
    let mut corrupt = valid.clone();
    corrupt[20] ^= 1;
    assert!(Png::parse(&corrupt).is_none());
    let mut overflow = valid.clone();
    overflow[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(Png::parse(&overflow).is_none());
    let mut trailing = valid;
    trailing.push(0);
    assert!(Png::parse(&trailing).is_none());
    for kind in [
        b"tRNS", b"acTL", b"fcTL", b"fdAT", b"PLTE", b"IHDR", b"ABCD", b"abca",
    ] {
        let mut data = envelope(17, 13, 6, 8, 0);
        data.extend(chunk(kind, &[]));
        assert!(Png::parse(&finish(data)).is_none(), "{kind:?}");
    }
    let mut separated = envelope(17, 13, 6, 8, 0);
    separated.extend(chunk(b"IDAT", &[]));
    separated.extend(chunk(b"tEXt", &[]));
    assert!(Png::parse(&finish(separated)).is_none());
    let mut oversized = finish(envelope(17, 13, 6, 8, 0));
    oversized.resize(MAX_ENCODED + 1, 0);
    assert!(Png::parse(&oversized).is_none());
}

#[test]
fn rejects_bad_zlib_headers_before_hardware() {
    for header in [[0x78, 0x9d], [0x78, 0x20], [0x79, 0x18], [0x88, 0x98]] {
        let mut data = envelope(17, 13, 6, 8, 0);
        data.extend(chunk(b"IDAT", &[header[0], header[1], 0, 0, 0, 0, 0]));
        data.extend(chunk(b"IEND", &[]));
        assert!(Png::parse(&data).is_none());
    }
}

#[test]
fn only_unambiguous_finish_and_exact_output_allow_cpu_access() {
    let data = finish(envelope(17, 13, 6, 8, 0));
    let png = Png::parse(&data).unwrap();
    assert!(completed(1, 932, &png));
    for status in [0, 2, 3, 4, 5, 8, 9, 0x10001] {
        assert!(!completed(status, 932, &png));
    }
    for count in [0, 884, 931, 933, 936, u32::MAX] {
        assert!(!completed(1, count, &png));
    }
}

#[test]
fn rgba_conversion_matches_slint_rounding_with_stride_and_all_alphas() {
    let data = finish(envelope(3, 256, 6, 8, 0));
    let png = Png::parse(&data).unwrap();
    let mut output = vec![0xa5; png.output_len()];
    let mut reference = SharedPixelBuffer::<Rgba8Pixel>::new(3, 256);
    for (i, pixel) in reference.make_mut_slice().iter_mut().enumerate() {
        *pixel = Rgba8Pixel::new(
            (i * 37) as u8,
            (i * 71) as u8,
            (i * 113) as u8,
            (i / 3) as u8,
        );
        let at = i / 3 * png.stride + i % 3 * 4;
        output[at..at + 4].copy_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
    }
    let expected = Image::from_rgba8(reference)
        .to_rgba8_premultiplied()
        .unwrap();
    assert_eq!(
        png.image(&output)
            .to_rgba8_premultiplied()
            .unwrap()
            .as_bytes(),
        expected.as_bytes()
    );
}

#[test]
fn rgb_conversion_excludes_padding_and_retains_rgb_storage() {
    let data = finish(envelope(1, 2, 2, 8, 0));
    let png = Png::parse(&data).unwrap();
    let output = [1, 2, 3, 255, 99, 98, 97, 96, 4, 5, 6, 255, 95, 94, 93, 92];
    assert_eq!(
        png.image(&output).to_rgb8().unwrap().as_bytes(),
        [1, 2, 3, 4, 5, 6]
    );
}
