use crate::{
    crypto::CipherId,
    error::Error,
    io::{Read, Write},
};
use alloc::{
    string::String,
    vec::Vec,
};
use byteorder::{BigEndian, ByteOrder};

pub const MAGIC_NUMBER: &[u8; 4] = b"RSTR";
pub const CURRENT_VERSION: u16 = 0x0001;

/// 索引表中的单个分块表项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub offset: u64,
    pub size: u64,
    pub nonce: [u8; 12],
}

/// Rstr 文件头结构
#[derive(Debug, Clone, PartialEq)]
pub struct RstrHeader {
    pub version: u16,
    pub cipher_id: CipherId,
    pub is_split: bool,
    pub mime_type: String,
    pub duration: f64,
    pub key_salt: [u8; 16],
    pub index_table: Vec<IndexEntry>,
}

impl RstrHeader {
    /// 序列化头部信息到二进制 Buffer
    pub fn serialize<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        // 1. 魔数 (4B)
        writer.write_all(MAGIC_NUMBER)?;

        // 2. 版本号 (2B)
        let mut buf2 = [0u8; 2];
        BigEndian::write_u16(&mut buf2, self.version);
        writer.write_all(&buf2)?;

        // 3. 算法标识符 (1B)
        let cipher_byte = self.cipher_id as u8;
        writer.write_all(&[cipher_byte])?;

        // 4. 分片标记字段 (1B): 0x01 表示物理分片多文件，0x00 表示单密文文件
        let split_byte = if self.is_split { 0x01 } else { 0x00 };
        writer.write_all(&[split_byte])?;

        // 5. MIME 类型 (1B 长度 + N 字节 UTF-8)
        let mime_bytes = self.mime_type.as_bytes();
        let mime_len = mime_bytes.len();
        if mime_len > 255 {
            return Err(Error::MimeTypeTooLong);
        }
        writer.write_all(&[mime_len as u8])?;
        writer.write_all(mime_bytes)?;

        // 6. 密钥盐值 (16B)
        writer.write_all(&self.key_salt)?;

        // 7. 视频总时长 (8B f64)
        let mut buf8 = [0u8; 8];
        BigEndian::write_f64(&mut buf8, self.duration);
        writer.write_all(&buf8)?;

        // 8. 索引表长度 (4B)
        let table_len = self.index_table.len() as u32;
        let mut buf4 = [0u8; 4];
        BigEndian::write_u32(&mut buf4, table_len);
        writer.write_all(&buf4)?;

        // 9. 索引表 (N * 28B)
        for entry in &self.index_table {
            let mut entry_buf = [0u8; 28];
            BigEndian::write_u64(&mut entry_buf[0..8], entry.offset);
            BigEndian::write_u64(&mut entry_buf[8..16], entry.size);
            entry_buf[16..28].copy_from_slice(&entry.nonce);
            writer.write_all(&entry_buf)?;
        }

        Ok(())
    }

    /// 从二进制 Reader 中解析反序列化头部信息
    pub fn deserialize<R: Read>(mut reader: R) -> Result<Self, Error> {
        // 1. 魔数 (4B)
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC_NUMBER {
            return Err(Error::InvalidMagicNumber);
        }

        // 2. 版本号 (2B)
        let mut version_buf = [0u8; 2];
        reader.read_exact(&mut version_buf)?;
        let version = BigEndian::read_u16(&version_buf);

        // 3. 算法标识符 (1B)
        let mut cipher_buf = [0u8; 1];
        reader.read_exact(&mut cipher_buf)?;
        let cipher_id = CipherId::try_from(cipher_buf[0])?;

        // 4. 分片标记字段 (1B)
        let mut split_buf = [0u8; 1];
        reader.read_exact(&mut split_buf)?;
        let is_split = split_buf[0] == 0x01;

        // 5. MIME 类型 (1B 长度 + N 字节 UTF-8)
        let mut mime_len_buf = [0u8; 1];
        reader.read_exact(&mut mime_len_buf)?;
        let mime_len = mime_len_buf[0] as usize;
        let mut mime_bytes = alloc::vec![0u8; mime_len];
        reader.read_exact(&mut mime_bytes)?;
        let mime_type = String::from_utf8(mime_bytes)
            .map_err(|_| Error::Io(String::from("Invalid UTF-8 in MIME type")))?;

        // 6. 密钥盐值 (16B)
        let mut key_salt = [0u8; 16];
        reader.read_exact(&mut key_salt)?;

        // 7. 视频总时长 (8B)
        let mut duration_buf = [0u8; 8];
        reader.read_exact(&mut duration_buf)?;
        let duration = BigEndian::read_f64(&duration_buf);

        // 8. 索引表长度 (4B)
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let table_len = BigEndian::read_u32(&len_buf) as usize;

        // 9. 索引表 (N * 28B)
        let mut index_table = Vec::with_capacity(table_len);
        for _ in 0..table_len {
            let mut entry_buf = [0u8; 28];
            reader.read_exact(&mut entry_buf)?;

            let offset = BigEndian::read_u64(&entry_buf[0..8]);
            let size = BigEndian::read_u64(&entry_buf[8..16]);
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&entry_buf[16..28]);

            index_table.push(IndexEntry {
                offset,
                size,
                nonce,
            });
        }

        Ok(RstrHeader {
            version,
            cipher_id,
            is_split,
            mime_type,
            duration,
            key_salt,
            index_table,
        })
    }

    /// 辅助方法：通过原始字节位置定位对应的加密分块索引及偏移范围
    pub fn locate_chunk(&self, byte_position: u64) -> Result<(usize, &IndexEntry), Error> {
        let mut current_unenc_offset = 0u64;
        for (idx, entry) in self.index_table.iter().enumerate() {
            if entry.size < 16 {
                return Err(Error::DataTooShort);
            }
            let unenc_size = entry.size - 16;
            let next_unenc_offset = current_unenc_offset + unenc_size;
            if byte_position >= current_unenc_offset && byte_position < next_unenc_offset {
                return Ok((idx, entry));
            }
            current_unenc_offset = next_unenc_offset;
        }

        Err(Error::OutOfBounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_header_serialize_deserialize() {
        let entry1 = IndexEntry {
            offset: 0,
            size: 5000,
            nonce: [1u8; 12],
        };
        let entry2 = IndexEntry {
            offset: 0,
            size: 5000,
            nonce: [2u8; 12],
        };

        let original_header = RstrHeader {
            version: CURRENT_VERSION,
            cipher_id: CipherId::Aes256Gcm,
            is_split: false,
            mime_type: String::from("video/mp4; codecs=\"avc1.64001e, mp4a.40.2\""),
            duration: 0.0,
            key_salt: [7u8; 16],
            index_table: vec![entry1, entry2],
        };

        let mut buffer = Vec::new();
        original_header.serialize(&mut buffer).unwrap();

        let deserialized_header = RstrHeader::deserialize(&buffer[..]).unwrap();
        assert_eq!(original_header, deserialized_header);
    }

    #[test]
    fn test_locate_chunk() {
        let entry1 = IndexEntry {
            offset: 0,
            size: 1016, // 1000字节明文 + 16字节 tag
            nonce: [1u8; 12],
        };
        let entry2 = IndexEntry {
            offset: 0,
            size: 1016, // 1000字节明文 + 16字节 tag
            nonce: [2u8; 12],
        };

        let header = RstrHeader {
            version: CURRENT_VERSION,
            cipher_id: CipherId::ChaCha20Poly1305,
            is_split: false,
            mime_type: String::from("video/mp4"),
            duration: 0.0,
            key_salt: [0u8; 16],
            index_table: vec![entry1.clone(), entry2.clone()],
        };

        // 原始字节位置在 0~999 应该属于分块 0
        let (idx1, item1) = header.locate_chunk(500).unwrap();
        assert_eq!(idx1, 0);
        assert_eq!(item1.offset, 0);

        // 原始字节位置在 1000~1999 应该属于分块 1
        let (idx2, item2) = header.locate_chunk(1500).unwrap();
        assert_eq!(idx2, 1);
        assert_eq!(item2.offset, 0);

        // 越界的情况
        assert!(header.locate_chunk(2000).is_err());

        // 测试变长分片寻道定位
        let var_entry1 = IndexEntry {
            offset: 0,
            size: 60, // 44字节明文 + 16字节 tag (初始化段)
            nonce: [1u8; 12],
        };
        let var_entry2 = IndexEntry {
            offset: 0,
            size: 1040, // 1024字节明文 + 16字节 tag (分片 1)
            nonce: [2u8; 12],
        };
        let var_header = RstrHeader {
            version: CURRENT_VERSION,
            cipher_id: CipherId::ChaCha20Poly1305,
            is_split: true,
            mime_type: String::from("video/mp4"),
            duration: 0.0,
            key_salt: [0u8; 16],
            index_table: vec![var_entry1, var_entry2],
        };

        // 字节 20 应该在初始化段 (分块 0)
        let (v_idx1, _) = var_header.locate_chunk(20).unwrap();
        assert_eq!(v_idx1, 0);

        // 字节 44 应该在分片 1 (分块 1)
        let (v_idx2, _) = var_header.locate_chunk(44).unwrap();
        assert_eq!(v_idx2, 1);

        // 字节 1000 应该在分片 1 (分块 1)
        let (v_idx3, _) = var_header.locate_chunk(1000).unwrap();
        assert_eq!(v_idx3, 1);

        // 越界 (44 + 1024 = 1068)
        assert!(var_header.locate_chunk(1068).is_err());
    }
}
