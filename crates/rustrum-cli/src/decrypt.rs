use crate::{
    encrypt::get_chunk_path,
    io_util::{IoReadAdapter, SliceRangeAllocator},
};
use rpassword::prompt_password;
use rustrum_core::{
    crypto::{decrypt_chunk_in_place, derive_key},
    header::RstrHeader,
};
use memmap2::{Mmap, MmapMut};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::PathBuf,
};

pub fn run_decrypt(
    input: PathBuf,
    output: PathBuf,
    password: Option<String>,
    threads: Option<usize>,
) -> Result<(), String> {
    let pwd = match password {
        Some(p) => p,
        None => prompt_password("请输入解密密码: ").map_err(|e| format!("读取密码失败: {}", e))?,
    };

    let mut rstrm_path = input.clone();
    rstrm_path.set_extension("rstrm");

    // 1. 读取并解析元数据头部
    let mut rstrm_file =
        File::open(&rstrm_path).map_err(|e| format!("打开元数据文件失败: {}", e))?;
    let mut read_adapter = IoReadAdapter(&mut rstrm_file);
    let header = RstrHeader::deserialize(&mut read_adapter)
        .map_err(|e| format!("解析元数据文件失败: {}", e))?;

    // 2. 派生密钥
    let derived_key = derive_key(pwd.as_bytes(), &header.key_salt)
        .map_err(|e| format!("派生解密密钥失败: {}", e))?;

    // 3. 从元数据中读取是否为多文件物理分片模式
    let is_split = header.is_split;

    // 4. 内存映射输入文件（若为单文件模式）
    let in_mmap = if !is_split {
        let file = File::open(&input)
            .map_err(|e| format!("打开加密文件失败: {}", e))?;
        let len = file.metadata().map_err(|e| format!("读取加密文件属性失败: {}", e))?.len();
        if len == 0 {
            return Err("错误: 加密视频文件大小为 0 字节。".to_string());
        }
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| format!("对加密文件进行内存映射失败: {}", e))?;
        Some(mmap)
    } else {
        None
    };

    // 5. 创建并内存映射解密输出文件
    let total_decrypted_size: u64 = header.index_table.iter().map(|entry| entry.size.saturating_sub(16)).sum();
    if total_decrypted_size == 0 {
        return Err("错误: 解密后的视频文件大小为 0 字节。".to_string());
    }
    let out_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&output)
        .map_err(|e| format!("创建解密输出文件失败: {}", e))?;
    out_file.set_len(total_decrypted_size)
        .map_err(|e| format!("设置输出文件大小失败: {}", e))?;
    let mut out_mmap = unsafe { MmapMut::map_mut(&out_file) }
        .map_err(|e| format!("对解密输出文件进行内存映射失败: {}", e))?;

    // 5. 计算各分片在解密输出文件中的偏移量并分配 slice ranges
    let mut ranges = Vec::with_capacity(header.index_table.len());
    let mut current_decrypted_offset = 0usize;
    for entry in &header.index_table {
        let size = entry.size.saturating_sub(16) as usize;
        ranges.push((current_decrypted_offset, size));
        current_decrypted_offset += size;
    }

    let allocator = SliceRangeAllocator::new(&mut out_mmap);
    let mut out_slices = allocator.allocate_ranges(&ranges)?;

    // 6. 逐片并发读取解密并写入输出映射文件
    let stem = input
        .file_stem()
        .ok_or_else(|| "无法获取输入文件主文件名".to_string())?
        .to_string_lossy();

    let process_chunks = |out_slices: &mut [&mut [u8]]| -> Result<(), String> {
        use rayon::prelude::*;

        header.index_table
            .par_iter()
            .zip(out_slices.par_iter_mut())
            .enumerate()
            .try_for_each(|(i, (entry, out_slice))| -> Result<(), String> {
                let mut chunk_buffer = vec![0u8; entry.size as usize];

                if is_split {
                    let chunk_path = get_chunk_path(&input, &stem, i);

                    let mut in_chunk_file = File::open(&chunk_path)
                        .map_err(|e| format!("打开分片文件 {:?} 失败: {}", chunk_path, e))?;

                    in_chunk_file
                        .read_exact(&mut chunk_buffer)
                        .map_err(|e| format!("读取加密分片数据失败: {}", e))?;
                } else {
                    if let Some(ref mmap) = in_mmap {
                        let start = entry.offset as usize;
                        let end = (entry.offset + entry.size) as usize;
                        chunk_buffer.copy_from_slice(&mmap[start..end]);
                    } else {
                        return Err("输入文件未进行内存映射".to_string());
                    }
                }

                let decrypted_slice = decrypt_chunk_in_place(
                    header.cipher_id,
                    &derived_key,
                    &entry.nonce,
                    &mut chunk_buffer,
                )
                .map_err(|e| format!("解密分片 {} 失败（可能是密码错误或数据损坏）: {}", i, e))?;

                out_slice.copy_from_slice(decrypted_slice);

                Ok(())
            })
    };

    if let Some(t) = threads {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build()
            .map_err(|e| format!("创建线程池失败: {}", e))?;
        pool.install(|| process_chunks(&mut out_slices))?;
    } else {
        process_chunks(&mut out_slices)?;
    }

    println!("成功解密视频并输出至: {:?}", output);
    Ok(())
}
