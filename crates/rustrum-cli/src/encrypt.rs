use crate::{
    ffmpeg::{Fmp4Detector, is_fragmented_mp4, probe_video_info},
    io_util::{IoWriteAdapter, SliceRangeAllocator},
};
use rand::{RngExt, rng};
use rpassword::prompt_password;
use rustrum_core::{
    crypto::{CipherId, derive_key, encrypt_chunk_in_place},
    header::{CURRENT_VERSION, IndexEntry, RstrHeader},
};
use memmap2::{Mmap, MmapMut};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn get_chunk_path(base_output: &Path, stem: &str, index: usize) -> PathBuf {
    let mut chunk_path = base_output.to_path_buf();
    chunk_path.set_file_name(format!("{}_{}.rstr", stem, index));
    chunk_path
}

pub struct EncryptOptions {
    pub input: PathBuf,
    pub output: PathBuf,
    pub name: Option<String>,
    pub password: Option<String>,
    pub cipher: CipherId,
    pub split: bool,
    pub threads: Option<usize>,
    pub quiet: bool,
}

pub fn run_encrypt(options: EncryptOptions) -> Result<(), String> {
    let EncryptOptions {
        input,
        output,
        name,
        password,
        cipher,
        split,
        threads,
        quiet,
    } = options;
    let pwd = match password {
        Some(p) => p,
        None => prompt_password("请输入加密密码: ").map_err(|e| format!("读取密码失败: {}", e))?,
    };

    let final_input = input.clone();

    // 1. 检查并确保输入为 FMP4
    let is_fmp4 = is_fragmented_mp4(&final_input);
    if !is_fmp4 {
        return Err("错误: 输入视频不是 Fragmented MP4 (FMP4) 格式。".to_string());
    }

    // 2. 密钥派生
    let mut salt = [0u8; 16];
    rng().fill(&mut salt);
    let derived_key =
        derive_key(pwd.as_bytes(), &salt).map_err(|e| format!("派生加密密钥失败: {}", e))?;

    // 3. 构建索引表与头部
    let file_meta =
        fs::metadata(&final_input).map_err(|e| format!("读取输入文件属性失败: {}", e))?;
    let total_size = file_meta.len();
    if total_size == 0 {
        return Err("错误: 输入视频文件大小为 0 字节。".to_string());
    }


    let detector =
        Fmp4Detector::open(&final_input).map_err(|e| format!("分析视频文件结构失败: {}", e))?;
    let fragments = detector.get_fragment_boundaries(total_size);
    let num_chunks = fragments.len();

    let mut index_table = Vec::with_capacity(num_chunks);
    let mut current_offset = 0u64;

    for &(_start_offset, length) in &fragments {
        let mut nonce = [0u8; 12];
        rng().fill(&mut nonce);

        let enc_len = length + 16; // 16 字节 tag

        if split {
            index_table.push(IndexEntry {
                offset: 0, // 独立文件时，分片文件内的偏移量恒定为 0
                size: enc_len,
                nonce,
            });
        } else {
            index_table.push(IndexEntry {
                offset: current_offset,
                size: enc_len,
                nonce,
            });
            current_offset += enc_len;
        }
    }
    let (video_duration, mime) = probe_video_info(&final_input)
        .map_err(|e| format!("使用 ffprobe 获取媒体信息失败: {}", e))?;
    let header = RstrHeader {
        version: CURRENT_VERSION,
        cipher_id: cipher,
        is_split: split,
        mime_type: mime,
        duration: video_duration,
        key_salt: salt,
        index_table: index_table.clone(),
    };

    // 提取或生成主文件名 stem
    let stem = match name {
        Some(n) => n,
        None => input
            .file_stem()
            .ok_or_else(|| "无法获取输入文件主文件名".to_string())?
            .to_string_lossy()
            .into_owned(),
    };

    // 确保输出目录存在
    if !output.exists() {
        fs::create_dir_all(&output)
            .map_err(|e| format!("创建输出目录 {:?} 失败: {}", output, e))?;
    } else if !output.is_dir() {
        return Err(format!("输出路径 {:?} 已存在且不是一个目录", output));
    }

    let rstr_path = output.join(format!("{}.rstr", stem));
    let rstrm_path = output.join(format!("{}.rstrm", stem));

    // 4. 打开元数据输出文件并序列化头部
    let mut out_rstrm =
        File::create(&rstrm_path).map_err(|e| format!("创建元数据文件失败: {}", e))?;
    let mut write_adapter = IoWriteAdapter(&mut out_rstrm);
    header
        .serialize(&mut write_adapter)
        .map_err(|e| format!("写入元数据文件失败: {}", e))?;

    // 5. 分片加密并写入输出
    // 内存映射输入 file
    let in_file = File::open(&final_input)
        .map_err(|e| format!("打开输入文件失败: {}", e))?;
    let in_mmap = unsafe { Mmap::map(&in_file) }
        .map_err(|e| format!("对输入文件进行内存映射失败: {}", e))?;

    // 如果是单物理文件模式，预先创建并映射输出文件
    let mut single_out_mmap = if !split {
        let total_output_size: u64 = index_table.iter().map(|entry| entry.size).sum();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&rstr_path)
            .map_err(|e| format!("创建加密文件失败: {}", e))?;
        file.set_len(total_output_size)
            .map_err(|e| format!("设置输出文件大小失败: {}", e))?;
        let mmap = unsafe { MmapMut::map_mut(&file) }
            .map_err(|e| format!("对输出文件进行内存映射失败: {}", e))?;
        Some(mmap)
    } else {
        None
    };

    let out_slices = if let Some(ref mut mmap) = single_out_mmap {
        let mut ranges = Vec::with_capacity(num_chunks);
        for entry in &index_table {
            ranges.push((entry.offset as usize, entry.size as usize));
        }
        let allocator = SliceRangeAllocator::new(mmap);
        Some(allocator.allocate_ranges(&ranges)?)
    } else {
        None
    };

    let process_chunks = |out_slices: Option<Vec<&mut [u8]>>| -> Result<(), String> {
        use rayon::prelude::*;

        let mut out_slices = out_slices.unwrap_or_else(|| {
            let mut v = Vec::with_capacity(num_chunks);
            for _ in 0..num_chunks {
                v.push(&mut [] as &mut [u8]);
            }
            v
        });

        (0..num_chunks)
            .into_par_iter()
            .zip(out_slices.par_iter_mut())
            .try_for_each(|(i, out_slice)| -> Result<(), String> {
                let entry = &index_table[i];
                let (start_offset, length) = fragments[i];

                let start = start_offset as usize;
                let end = (start_offset + length) as usize;
                let src_slice = &in_mmap[start..end];

                let mut chunk_data = vec![0u8; length as usize];
                chunk_data.copy_from_slice(src_slice);

                encrypt_chunk_in_place(
                    header.cipher_id,
                    &derived_key,
                    &entry.nonce,
                    &mut chunk_data,
                )
                .map_err(|e| format!("加密分片 {} 失败: {}", i, e))?;

                if split {
                    let chunk_path = get_chunk_path(&rstr_path, &stem, i);
                    let mut out_chunk_file = File::create(&chunk_path)
                        .map_err(|e| format!("创建分片文件 {:?} 失败: {}", chunk_path, e))?;
                    out_chunk_file
                        .write_all(&chunk_data)
                        .map_err(|e| format!("写入分片加密数据失败: {}", e))?;
                } else {
                    out_slice.copy_from_slice(&chunk_data);
                }

                Ok(())
            })
    };

    if let Some(t) = threads {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build()
            .map_err(|e| format!("创建线程池失败: {}", e))?;
        pool.install(|| process_chunks(out_slices))?;
    } else {
        process_chunks(out_slices)?;
    }

    if !quiet {
        if split {
            println!("成功加密视频分片（多独立物理分段模式）！");
        } else {
            println!("成功加密视频分片（单文件模式）！");
        }
        println!("元数据已输出至: {:?}", rstrm_path);

        println!("--- 元数据信息 ---");
        println!("版本号: 0x{:04X}", header.version);
        println!("加密算法: {:?}", header.cipher_id);
        println!("物理分片: {}", if header.is_split { "是" } else { "否" });
        println!("MIME 类型: {}", header.mime_type);
        println!("视频时长: {:.3} 秒", header.duration);
        let salt_hex: String = header.key_salt.iter().map(|b| format!("{:02x}", b)).collect();
        println!("密钥盐值 (HEX): {}", salt_hex);
        println!("分片总数: {}", header.index_table.len());
        println!("----------------");
    }

    Ok(())
}
