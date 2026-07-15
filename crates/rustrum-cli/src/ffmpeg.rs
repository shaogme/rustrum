use serde::Deserialize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::Command,
};

pub struct Fmp4Detector {
    has_moof: bool,
    has_styp: bool,
    compatible_brands: Vec<[u8; 4]>,
    moof_offsets: Vec<u64>,
}

impl Fmp4Detector {
    /// 打开并解析视频文件的顶层 Box 结构
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
        let file_len = file
            .metadata()
            .map_err(|e| format!("无法获取文件属性: {}", e))?
            .len();

        let mut reader = file;
        let mut offset = 0;
        let mut has_moof = false;
        let mut has_styp = false;
        let mut compatible_brands = Vec::new();
        let mut moof_offsets = Vec::new();

        while offset < file_len {
            if reader.seek(SeekFrom::Start(offset)).is_err() {
                break;
            }

            let mut header = [0u8; 8];
            if reader.read_exact(&mut header).is_err() {
                break;
            }

            let mut box_size = u32::from_be_bytes(header[0..4].try_into().unwrap()) as u64;
            let box_type: [u8; 4] = header[4..8].try_into().unwrap();

            let mut header_size = 8;
            if box_size == 1 {
                let mut large_size_buf = [0u8; 8];
                if reader.read_exact(&mut large_size_buf).is_err() {
                    break;
                }
                box_size = u64::from_be_bytes(large_size_buf);
                header_size = 16;
            } else if box_size == 0 {
                box_size = file_len - offset;
            }

            if box_size < header_size {
                break;
            }

            // 防范恶意或损坏的大 Box Size 导致的内存溢出风险
            let payload_size = box_size.saturating_sub(header_size);
            let remaining_file = file_len.saturating_sub(offset + header_size);
            if payload_size > remaining_file {
                break;
            }

            // 检测 moof 盒并记录其在文件中的起始偏移量
            if &box_type == b"moof" {
                has_moof = true;
                moof_offsets.push(offset);
            }

            // 检测 styp 盒
            if &box_type == b"styp" {
                has_styp = true;
            }

            // 解析 ftyp 中的 compatible_brands
            if &box_type == b"ftyp" {
                if payload_size >= 8 && payload_size <= 1024 * 1024 {
                    // ftyp 限制在合理大小内
                    let mut ftyp_payload = vec![0u8; payload_size as usize];
                    if reader.read_exact(&mut ftyp_payload).is_ok() {
                        for chunk in ftyp_payload[8..].chunks_exact(4) {
                            if let Ok(brand) = chunk.try_into() {
                                compatible_brands.push(brand);
                            }
                        }
                    }
                }
            }

            offset += box_size;
        }

        let detector = Self {
            has_moof,
            has_styp,
            compatible_brands,
            moof_offsets,
        };

        if !detector.is_fmp4() {
            return Err("输入视频不是 Fragmented MP4 (FMP4) 格式".to_string());
        }

        Ok(detector)
    }

    /// 方法 1：检测是否存在 moof（Movie Fragment Box）
    pub fn detect_moof(&self) -> bool {
        self.has_moof
    }

    /// 方法 2：检测是否存在 styp（Segment Type Box）
    pub fn detect_styp(&self) -> bool {
        self.has_styp
    }

    /// 方法 3：解析 ftyp 盒子中的兼容品牌 (Compatible Brands) 并与已知 FMP4 兼容特征进行匹配
    pub fn detect_ftyp_brands(&self) -> bool {
        self.compatible_brands
            .iter()
            .any(|brand| matches!(brand, b"iso5" | b"iso6" | b"dash" | b"msdh" | b"cmfc"))
    }

    /// 综合三种方法进行 FMP4 的判定
    pub fn is_fmp4(&self) -> bool {
        self.detect_moof() || self.detect_styp() || self.detect_ftyp_brands()
    }

    /// 获取各个自然分片的区间：(起始偏移, 长度)
    pub fn get_fragment_boundaries(&self, file_len: u64) -> Vec<(u64, u64)> {
        if self.moof_offsets.is_empty() {
            // 如果没有 moof，则整个文件作为一个分段
            return vec![(0, file_len)];
        }

        let mut boundaries = Vec::with_capacity(self.moof_offsets.len() + 1);

        // 1. 初始化段：从 0 到第一个 moof 起点
        if let Some(&first_moof) = self.moof_offsets.first() {
            if first_moof > 0 {
                boundaries.push((0, first_moof));
            }
        }

        // 2. 中间的分段：从每个 moof 起点到下一个 moof 起点
        for win in self.moof_offsets.windows(2) {
            let start = win[0];
            let end = win[1];
            boundaries.push((start, end.saturating_sub(start)));
        }

        // 3. 最后一个分段：从最后一个 moof 起点到文件结束
        if let Some(&last_moof) = self.moof_offsets.last() {
            if file_len > last_moof {
                boundaries.push((last_moof, file_len - last_moof));
            }
        }

        boundaries
    }
}

/// 兼容原有 API 的辅助检测函数
pub fn is_fragmented_mp4<P: AsRef<Path>>(path: P) -> bool {
    Fmp4Detector::open(path)
        .map(|detector| detector.is_fmp4())
        .unwrap_or(false)
}

#[derive(Deserialize, Debug)]
struct FfprobeStream {
    codec_name: Option<String>,
    codec_type: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
}

#[derive(Deserialize, Debug)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Deserialize, Debug)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}

/// 使用 ffprobe 解析视频时长和带 codec 参数的 MIME 类型
pub fn probe_video_info<P: AsRef<Path>>(path: P) -> Result<(f64, String), String> {
    let path = path.as_ref();
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("启动 ffprobe 失败，请检查是否安装了 ffmpeg/ffprobe: {}", e))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe 执行失败: {}", err_msg));
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("解析 ffprobe 输出的 JSON 失败: {}", e))?;

    let duration = if let Some(ref fmt) = parsed.format {
        if let Some(ref dur_str) = fmt.duration {
            dur_str.parse::<f64>().map_err(|e| format!("解析视频时长失败: {}", e))?
        } else {
            return Err("ffprobe 未输出视频时长 (duration)".to_string());
        }
    } else {
        return Err("ffprobe 未输出 format 信息".to_string());
    };

    let streams = parsed.streams.unwrap_or_default();
    let mut video_codec = None;
    let mut audio_codec = None;

    for stream in streams {
        if let Some(ref codec_type) = stream.codec_type {
            if codec_type == "video" {
                if let Some(ref name) = stream.codec_name {
                    let codec_str = match name.as_str() {
                        "h264" => {
                            let profile = stream.profile.as_deref().unwrap_or("High");
                            let level = stream.level.unwrap_or(31);
                            let profile_hex = match profile {
                                "Constrained Baseline" => "42e0",
                                "Baseline" => "4200",
                                "Main" => "4d00",
                                _ => "6400",
                            };
                            let level_hex = format!("{:02x}", level);
                            format!("avc1.{}{}", profile_hex, level_hex)
                        }
                        "hevc" | "h265" => "hev1.1.6.L93.B0".to_string(),
                        "vp9" => "vp09.00.10.08".to_string(),
                        "vp8" => "vp8".to_string(),
                        "av1" => "av01.0.04M.08".to_string(),
                        other => other.to_string(),
                    };
                    video_codec = Some(codec_str);
                }
            } else if codec_type == "audio" {
                if let Some(ref name) = stream.codec_name {
                    let codec_str = match name.as_str() {
                        "aac" => "mp4a.40.2".to_string(),
                        "opus" => "opus".to_string(),
                        other => other.to_string(),
                    };
                    audio_codec = Some(codec_str);
                }
            }
        }
    }

    let base_mime = mime_guess::from_path(path)
        .first_raw()
        .ok_or_else(|| "无法获取输入视频的 MIME 类型".to_string())?
        .to_string();

    let codecs = match (video_codec, audio_codec) {
        (Some(v), Some(a)) => format!("{}; codecs=\"{}, {}\"", base_mime, v, a),
        (Some(v), None) => format!("{}; codecs=\"{}\"", base_mime, v),
        (None, Some(a)) => format!("{}; codecs=\"{}\"", base_mime, a),
        _ => base_mime,
    };

    Ok((duration, codecs))
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_fragmented_mp4_with_real_file() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // crates/
        path.pop(); // rustrum/
        path.push("data");
        path.push("fmp4.mp4");

        if path.exists() {
            let detector = Fmp4Detector::open(&path).expect("打开与解析真实文件失败");

            // 独立使用每种检测方法进行验证并断言
            let moof_result = detector.detect_moof();
            let styp_result = detector.detect_styp();
            let ftyp_brands_result = detector.detect_ftyp_brands();
            let is_fmp4_result = detector.is_fmp4();

            println!("--- FMP4 独立检测方法验证结果 (文件: {:?}) ---", path);
            println!("方法 1 (检测 moof): {}", moof_result);
            println!("方法 2 (检测 styp): {}", styp_result);
            println!("方法 3 (检测 ftyp 兼容品牌): {}", ftyp_brands_result);
            println!("综合判定结果: {}", is_fmp4_result);

            assert!(is_fmp4_result, "fmp4.mp4 综合判定应当为 true");

            assert!(
                moof_result || ftyp_brands_result,
                "至少有一种独立检测方法应当判定为 true"
            );

            // 测试原生分片划分
            let boundaries = detector.get_fragment_boundaries(3951507);
            println!("分片数量: {}", boundaries.len());
            for (idx, (offset, len)) in boundaries.iter().enumerate() {
                println!("分片 {}: 偏移 = {}, 长度 = {}", idx, offset, len);
            }
            assert!(boundaries.len() > 1, "自然分片数应该大于 1");
        }
    }
}
