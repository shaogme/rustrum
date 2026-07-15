use std::fs;
use std::process::Command;

#[test]
fn test_cli_encrypt_decrypt_flow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let input_path = temp_dir.path().join("input.mp4");
    let decrypted_path = temp_dir.path().join("decrypted.mp4");

    let project_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let real_mp4_path = project_dir.parent().unwrap().parent().unwrap().join("data").join("fmp4.mp4");
    
    // Copy the real fmp4 video to the temp directory
    fs::copy(&real_mp4_path, &input_path).unwrap();

    let original_data = fs::read(&input_path).unwrap();

    let ciphers = ["chacha20-poly1305", "aes256-gcm", "aes128-gcm"];
    let password = "test_secure_password";

    // 测试 1: 默认单加密合并文件模式
    for cipher in ciphers {
        let output_rstr = temp_dir.path().join("input.rstr");
        let output_rstrm = temp_dir.path().join("input.rstrm");
        if output_rstr.exists() {
            fs::remove_file(&output_rstr).unwrap();
        }
        if output_rstrm.exists() {
            fs::remove_file(&output_rstrm).unwrap();
        }
        if decrypted_path.exists() {
            fs::remove_file(&decrypted_path).unwrap();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "rustrum-cli",
                "--",
                "encrypt",
                "-i",
                input_path.to_str().unwrap(),
                "-o",
                temp_dir.path().to_str().unwrap(),
                "-p",
                password,
                "-c",
                cipher,
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "[单文件] 算法 {} 加密命令失败: {}",
            cipher,
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(output_rstr.exists(), "[单文件] 密文合并文件未生成");
        assert!(
            output_rstrm.exists(),
            "[单文件] 算法 {} 未生成元数据文件",
            cipher
        );

        // 调用解密
        let output_dec = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "rustrum-cli",
                "--",
                "decrypt",
                "-i",
                output_rstr.to_str().unwrap(),
                "-o",
                decrypted_path.to_str().unwrap(),
                "-p",
                password,
            ])
            .output()
            .unwrap();

        assert!(
            output_dec.status.success(),
            "[单文件] 算法 {} 解密命令失败: {}",
            cipher,
            String::from_utf8_lossy(&output_dec.stderr)
        );

        let decrypted_data = fs::read(&decrypted_path).unwrap();
        assert_eq!(
            original_data, decrypted_data,
            "[单文件] 算法 {} 解密数据不匹配",
            cipher
        );

        // 清理
        fs::remove_file(&output_rstr).unwrap();
        fs::remove_file(&output_rstrm).unwrap();
        fs::remove_file(&decrypted_path).unwrap();
    }

    // 测试 2: --split 物理分片多文件模式
    for cipher in ciphers {
        let chunk0_path = temp_dir.path().join("input_0.rstr");
        let chunk1_path = temp_dir.path().join("input_1.rstr");
        let metadata_path = temp_dir.path().join("input.rstrm");

        if chunk0_path.exists() {
            fs::remove_file(&chunk0_path).unwrap();
        }
        if chunk1_path.exists() {
            fs::remove_file(&chunk1_path).unwrap();
        }
        if decrypted_path.exists() {
            fs::remove_file(&decrypted_path).unwrap();
        }
        if metadata_path.exists() {
            fs::remove_file(&metadata_path).unwrap();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "rustrum-cli",
                "--",
                "encrypt",
                "-i",
                input_path.to_str().unwrap(),
                "-o",
                temp_dir.path().to_str().unwrap(),
                "-p",
                password,
                "-c",
                cipher,
                "--split",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "[物理分片] 算法 {} 加密命令失败: {}",
            cipher,
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(chunk0_path.exists(), "[物理分片] 分片 0 未生成");
        assert!(chunk1_path.exists(), "[物理分片] 分片 1 未生成");
        assert!(
            metadata_path.exists(),
            "[物理分片] 算法 {} 未生成元数据文件",
            cipher
        );

        let base_rstr_path = temp_dir.path().join("input.rstr");

        // 调用解密
        let output_dec = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "rustrum-cli",
                "--",
                "decrypt",
                "-i",
                base_rstr_path.to_str().unwrap(),
                "-o",
                decrypted_path.to_str().unwrap(),
                "-p",
                password,
            ])
            .output()
            .unwrap();

        assert!(
            output_dec.status.success(),
            "[物理分片] 算法 {} 解密命令失败: {}",
            cipher,
            String::from_utf8_lossy(&output_dec.stderr)
        );

        let decrypted_data = fs::read(&decrypted_path).unwrap();
        assert_eq!(
            original_data, decrypted_data,
            "[物理分片] 算法 {} 解密数据不匹配",
            cipher
        );

        // 清理
        fs::remove_file(&chunk0_path).unwrap();
        fs::remove_file(&chunk1_path).unwrap();
        fs::remove_file(&metadata_path).unwrap();
        fs::remove_file(&decrypted_path).unwrap();
    }
}

