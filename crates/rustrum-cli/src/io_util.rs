use rustrum_core::error::Error as CoreError;
use std::io::{Read, Write};


pub struct SliceRangeAllocator<'a> {
    slice: &'a mut [u8],
}

impl<'a> SliceRangeAllocator<'a> {
    pub fn new(slice: &'a mut [u8]) -> Self {
        Self { slice }
    }

    pub fn allocate_ranges(
        self,
        ranges: &[(usize, usize)],
    ) -> Result<Vec<&'a mut [u8]>, String> {
        let slice_len = self.slice.len();

        let mut sorted_ranges = Vec::with_capacity(ranges.len());
        for (idx, &(offset, size)) in ranges.iter().enumerate() {
            sorted_ranges.push((offset, size, idx));
        }
        sorted_ranges.sort_by_key(|r| r.0);

        let mut last_end = 0;
        for &(offset, size, idx) in &sorted_ranges {
            if offset < last_end {
                return Err(format!(
                    "重叠的分配范围: 范围 {} 与前一个范围重叠",
                    idx
                ));
            }
            let end = offset.checked_add(size).ok_or("分配范围溢出")?;
            if end > slice_len {
                return Err(format!(
                    "分配范围超出切片边界: 范围 {} 结束位置 {} 大于切片长度 {}",
                    idx, end, slice_len
                ));
            }
            last_end = end;
        }

        let mut allocated_slices = Vec::with_capacity(sorted_ranges.len());
        let mut remaining = self.slice;
        let mut current_cursor = 0;

        for &(offset, size, _) in &sorted_ranges {
            let gap = offset - current_cursor;
            if gap > 0 {
                let (_, rest) = remaining.split_at_mut(gap);
                remaining = rest;
            }
            let (target, rest) = remaining.split_at_mut(size);
            allocated_slices.push(target);
            remaining = rest;
            current_cursor = offset + size;
        }

        let mut final_result: Vec<&'a mut [u8]> = (0..ranges.len())
            .map(|_| &mut [] as &'a mut [u8])
            .collect();
        for (sorted_idx, &(_, _, original_idx)) in sorted_ranges.iter().enumerate() {
            final_result[original_idx] = std::mem::replace(&mut allocated_slices[sorted_idx], &mut []);
        }

        Ok(final_result)
    }
}

// 适配器将 std::io::Write 转换为 rustrum_core::io::Write
pub struct IoWriteAdapter<W>(pub W);

impl<W: Write> rustrum_core::io::Write for IoWriteAdapter<W> {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), CoreError> {
        self.0
            .write_all(buf)
            .map_err(|e| CoreError::Io(e.to_string()))
    }
}

// 适配器将 std::io::Read 转换为 rustrum_core::io::Read
pub struct IoReadAdapter<R>(pub R);

impl<R: Read> rustrum_core::io::Read for IoReadAdapter<R> {
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), CoreError> {
        self.0
            .read_exact(buf)
            .map_err(|e| CoreError::Io(e.to_string()))
    }
}
