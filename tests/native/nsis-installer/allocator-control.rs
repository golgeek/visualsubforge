// SPDX-License-Identifier: GPL-3.0-or-later
#[cfg(test)]
mod vsf_allocator_control {
    use super::*;

    #[test]
    fn alignment_zeroing_and_reallocation_preserve_the_layout() {
        for alignment in [1, 2, 4, 8, 16, 32, 64, 256, 4096] {
            for size in [1, 7, 16, 257] {
                let layout = Layout::from_size_align(size, alignment).unwrap();
                // Direct calls exercise the allocator contract without the
                // optimizer eliding allocations. Never dereference a failed
                // or misaligned result; release it before asserting failure.
                unsafe {
                    let pointer = Heapalloc.alloc_zeroed(layout);
                    assert!(!pointer.is_null());
                    let aligned = pointer as usize % alignment == 0;
                    if !aligned {
                        Heapalloc.dealloc(pointer, layout);
                        assert!(aligned, "alignment={alignment}, size={size}");
                    }
                    assert!(core::slice::from_raw_parts(pointer, size)
                        .iter()
                        .all(|byte| *byte == 0));
                    pointer.write_bytes(0x5a, size);
                    let grown = Heapalloc.realloc(pointer, layout, size + 513);
                    assert!(!grown.is_null());
                    assert_eq!(
                        grown as usize % alignment,
                        0,
                        "alignment={alignment}, size={size}, operation=grow"
                    );
                    assert!(core::slice::from_raw_parts(grown, size)
                        .iter()
                        .all(|byte| *byte == 0x5a));
                    let grown_layout = Layout::from_size_align(size + 513, alignment).unwrap();
                    let shrunk = Heapalloc.realloc(grown, grown_layout, 1);
                    assert!(!shrunk.is_null());
                    assert_eq!(
                        shrunk as usize % alignment,
                        0,
                        "alignment={alignment}, size={size}, operation=shrink"
                    );
                    assert_eq!(*shrunk, 0x5a);
                    Heapalloc.dealloc(shrunk, Layout::from_size_align(1, alignment).unwrap());
                }
            }
        }
    }
}
