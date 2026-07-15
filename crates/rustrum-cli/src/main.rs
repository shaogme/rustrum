mod decrypt;
mod encrypt;
mod ffmpeg;
mod io_util;

use clap::{Parser, Subcommand, ValueEnum};
use decrypt::run_decrypt;
use encrypt::{
    EncryptOptions,
    run_encrypt,
};
use rustrum_core::crypto::CipherId;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rustrum-cli")]
#[command(about = "Rustrum 安全视频流命令行加密与解密工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 加密视频文件为 .rstr 格式
    Encrypt {
        /// 输入视频文件路径
        #[arg(short, long)]
        input: PathBuf,

        /// 输出目录路径
        #[arg(short, long)]
        output: PathBuf,

        /// 输出文件名（可选，默认为输入文件名，不含后缀）
        #[arg(short, long)]
        name: Option<String>,

        /// 加密密码（若未指定，则会安全提示输入）
        #[arg(short, long)]
        password: Option<String>,

        /// 对称加密算法
        #[arg(short = 'c', long, value_enum, default_value_t = CliCipher::Chacha20Poly1305)]
        cipher: CliCipher,

        /// 是否独立物理分片存储
        #[arg(long)]
        split: bool,

        /// 并发线程数限制
        #[arg(short = 't', long)]
        threads: Option<usize>,

        /// 是否静默模式（不输出任何打印信息）
        #[arg(short, long)]
        quiet: bool,
    },
    /// 解密 .rstr 文件为原始视频格式
    Decrypt {
        /// 输入 .rstr 文件路径
        #[arg(short, long)]
        input: PathBuf,

        /// 输出视频文件路径
        #[arg(short, long)]
        output: PathBuf,

        /// 解密密码（若未指定，则会安全提示输入）
        #[arg(short, long)]
        password: Option<String>,

        /// 并发线程数限制
        #[arg(short = 't', long)]
        threads: Option<usize>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliCipher {
    Chacha20Poly1305,
    Aes256Gcm,
    Aes128Gcm,
}

impl From<CliCipher> for CipherId {
    fn from(c: CliCipher) -> Self {
        match c {
            CliCipher::Chacha20Poly1305 => CipherId::ChaCha20Poly1305,
            CliCipher::Aes256Gcm => CipherId::Aes256Gcm,
            CliCipher::Aes128Gcm => CipherId::Aes128Gcm,
        }
    }
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encrypt {
            input,
            output,
            name,
            password,
            cipher,
            split,
            threads,
            quiet,
        } => {
            run_encrypt(EncryptOptions {
                input,
                output,
                name,
                password,
                cipher: CipherId::from(cipher),
                split,
                threads,
                quiet,
            })?;
        }
        Commands::Decrypt {
            input,
            output,
            password,
            threads,
        } => {
            run_decrypt(input, output, password, threads)?;
        }
    }

    Ok(())
}
