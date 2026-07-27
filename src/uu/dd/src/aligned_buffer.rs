// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::io;

/// A fixed-size byte buffer with an aligned data address.
pub(crate) struct AlignedBuffer {
    storage: Vec<u8>,
    data_start: usize,
}

impl AlignedBuffer {
    pub(crate) fn try_new(len: usize, alignment: usize) -> io::Result<Self> {
        if !alignment.is_power_of_two() {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        let allocation_len = len
            .checked_add(alignment - 1)
            .ok_or(io::ErrorKind::OutOfMemory)?;
        let mut storage: Vec<u8> = Vec::new();
        storage.try_reserve_exact(allocation_len)?;

        let data_start = storage.as_ptr().align_offset(alignment);
        if data_start == usize::MAX {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        storage.resize(data_start + len, 0);

        Ok(Self {
            storage,
            data_start,
        })
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.storage[self.data_start..]
    }

    /// Copies `src` into the buffer and returns the aligned view of it.
    ///
    /// The buffer grows to fit `src` and keeps its allocation for later
    /// calls, so a converted block can be handed to a descriptor opened
    /// with `O_DIRECT` without allocating one buffer per block.
    pub(crate) fn refill(&mut self, src: &[u8], alignment: usize) -> io::Result<&[u8]> {
        let needed = src
            .len()
            .checked_add(alignment - 1)
            .ok_or(io::ErrorKind::OutOfMemory)?;
        if self.storage.capacity() < needed {
            self.storage.clear();
            self.storage
                .try_reserve_exact(needed - self.storage.len())?;
            self.data_start = self.storage.as_ptr().align_offset(alignment);
        }
        self.storage.clear();
        self.storage.resize(self.data_start, 0);
        self.storage.extend_from_slice(src);
        Ok(&self.storage[self.data_start..])
    }
}

#[cfg(test)]
mod tests {
    use super::AlignedBuffer;
    use std::io;

    #[test]
    fn test_alignment_and_writes() -> io::Result<()> {
        for alignment in [1, 2, 8, 512, 4096, 65536] {
            for len in [0, 1, 511, 512, 4097] {
                let mut buffer = AlignedBuffer::try_new(len, alignment)?;
                let data = buffer.as_mut_slice();
                assert_eq!(data.as_ptr().addr() % alignment, 0);
                assert_eq!(data.len(), len);
                assert!(data.iter().all(|&byte| byte == 0));
                data.fill(0xA5);
                assert!(data.iter().all(|&byte| byte == 0xA5));
            }
        }
        Ok(())
    }

    #[test]
    fn test_refill_keeps_the_data_aligned() -> io::Result<()> {
        for alignment in [1, 512, 4096] {
            let mut buffer = AlignedBuffer::try_new(0, alignment)?;
            // Grow, shrink and grow again: a reused buffer must stay aligned
            // even when an earlier payload forced a reallocation.
            for len in [1, 8192, 512, 65536, 0] {
                let payload = vec![0x5A; len];
                let data = buffer.refill(&payload, alignment)?;
                assert_eq!(data.len(), len);
                assert_eq!(data.as_ptr().addr() % alignment, 0);
                assert!(data.iter().all(|&byte| byte == 0x5A));
            }
        }
        Ok(())
    }

    #[test]
    fn test_rejects_invalid_parameters() {
        for alignment in [0, 3] {
            assert!(matches!(
                AlignedBuffer::try_new(1, alignment),
                Err(error) if error.kind() == io::ErrorKind::InvalidInput
            ));
        }
        assert!(matches!(
            AlignedBuffer::try_new(usize::MAX, 4096),
            Err(error) if error.kind() == io::ErrorKind::OutOfMemory
        ));
    }
}
