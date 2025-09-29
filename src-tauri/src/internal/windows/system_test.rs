/// 系统级测试 - 测试真实微信进程的密钥提取
/// 
/// 这个测试需要：
/// 1. 微信正在运行
/// 2. 提供有效的微信数据库文件路径
/// 3. 在Windows上可能需要管理员权限
/// 4. 在macOS上需要禁用SIP

use wechat_key_extractor::{
    KeyExtractor, 
    process::create_detector, 
    validator::Validator,
    version,
    Error, Result
};
use std::path::Path;
use std::env;
use log::{info, error, warn, debug};

/// 系统级密钥提取测试
/// 
/// 使用方法：
/// ```
/// RUST_LOG=debug cargo test --release test_real_wechat_key_extraction -- --nocapture --ignored
/// ```
#[tokio::test]
#[ignore] // 默认忽略，需要显式运行
async fn test_real_wechat_key_extraction() {
    // 初始化日志
    env_logger::try_init().ok();
    
    println!("\n=== 开始真实微信密钥提取测试 ===");
    
    // 检查是否有微信进程运行
    let detector = create_detector();
    let processes = match detector.find_processes() {
        Ok(processes) => processes,
        Err(e) => {
            eprintln!("❌ 无法查找微信进程: {:?}", e);
            eprintln!("请确保微信正在运行");
            return;
        }
    };
    
    if processes.is_empty() {
        eprintln!("❌ 未找到微信进程");
        eprintln!("请启动微信后重新运行测试");
        return;
    }
    
    println!("✅ 找到 {} 个微信进程:", processes.len());
    
    for (i, process) in processes.iter().enumerate() {
        println!("  {}. PID: {}, 版本: {}, 状态: {}", 
                i + 1,
                process.pid,
                process.version,
                if process.is_online() { "在线" } else { "离线" }
        );
        
        if let Some(ref exe_path) = process.exe_path {
            println!("     路径: {}", exe_path);
        }
        
        if let Some(ref data_dir) = process.data_dir {
            println!("     数据目录: {}", data_dir);
        }
    }
    
    // 选择第一个在线的进程
    let target_process = match processes.into_iter().find(|p| p.is_online()) {
        Some(process) => process,
        None => {
            eprintln!("❌ 未找到在线的微信进程");
            return;
        }
    };
    
    println!("\n📱 使用进程: PID {}, 版本 {}", target_process.pid, target_process.version);
    
    // 尝试找到数据库文件
    let db_path = find_database_file(&target_process).await;
    
    match db_path {
        Some(path) => {
            println!("📄 找到数据库文件: {}", path.display());
            
            // 执行密钥提取测试
            match test_key_extraction_with_db(&target_process, &path).await {
                Ok(_) => println!("✅ 密钥提取测试成功完成!"),
                Err(e) => {
                    eprintln!("❌ 密钥提取测试失败: {:?}", e);
                    print_troubleshooting_tips(&e);
                }
            }
        }
        None => {
            eprintln!("❌ 未找到数据库文件");
            println!("\n💡 手动指定数据库文件路径的方法:");
            println!("设置环境变量 WECHAT_DB_PATH，例如:");
            if cfg!(windows) {
                println!(r#"  set WECHAT_DB_PATH=C:\Users\YourName\Documents\WeChat Files\YourWxId\Msg\MSG0.db"#);
            } else {
                println!(r#"  export WECHAT_DB_PATH="/Users/YourName/Library/Containers/com.tencent.xinWeChat/Data/Library/Application Support/com.tencent.xinWeChat/2.0b4.0.9/YourWxId/Message/MSG0.db""#);
            }
            
            // 尝试从环境变量获取数据库路径
            if let Ok(env_path) = env::var("WECHAT_DB_PATH") {
                let path = Path::new(&env_path);
                if path.exists() {
                    println!("📄 使用环境变量指定的数据库: {}", path.display());
                    match test_key_extraction_with_db(&target_process, path).await {
                        Ok(_) => println!("✅ 密钥提取测试成功完成!"),
                        Err(e) => {
                            eprintln!("❌ 密钥提取测试失败: {:?}", e);
                            print_troubleshooting_tips(&e);
                        }
                    }
                } else {
                    eprintln!("❌ 环境变量指定的数据库文件不存在: {}", path.display());
                }
            }
        }
    }
    
    println!("\n=== 真实微信密钥提取测试结束 ===");
}

/// 尝试找到微信数据库文件
async fn find_database_file(process: &wechat_key_extractor::process::WeChatProcess) -> Option<std::path::PathBuf> {
    debug!("尝试查找微信数据库文件...");
    
    let possible_paths = if cfg!(windows) {
        // Windows可能的路径
        vec![
            // 从进程数据目录开始查找
            process.data_dir.as_ref().map(|dir| Path::new(dir).join("Msg").join("MSG0.db")),
            
            // 常见的Windows路径模式
            dirs::home_dir().map(|home| home.join("Documents").join("WeChat Files")),
        ]
    } else if cfg!(target_os = "macos") {
        // macOS可能的路径
        vec![
            dirs::home_dir().map(|home| 
                home.join("Library")
                    .join("Containers")
                    .join("com.tencent.xinWeChat")
                    .join("Data")
                    .join("Library")
                    .join("Application Support")
                    .join("com.tencent.xinWeChat")
            ),
        ]
    } else {
        // Linux路径（如果支持的话）
        vec![]
    };
    
    for path_option in possible_paths {
        if let Some(base_path) = path_option {
            debug!("检查路径: {:?}", base_path);
            
            if base_path.exists() {
                // 递归搜索MSG0.db文件
                if let Some(db_file) = search_msg_db(&base_path).await {
                    return Some(db_file);
                }
            }
        }
    }
    
    None
}

/// 递归搜索MSG0.db文件
async fn search_msg_db(dir: &Path) -> Option<std::path::PathBuf> {
    use std::fs;
    
    debug!("搜索目录: {:?}", dir);
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            if path.is_file() && path.file_name().map_or(false, |name| name == "MSG0.db") {
                println!("找到数据库文件: {:?}", path);
                return Some(path);
            } else if path.is_dir() {
                // 递归搜索子目录（限制深度以避免无限递归）
                if let Some(db_file) = search_msg_db(&path).await {
                    return Some(db_file);
                }
            }
        }
    }
    
    None
}

/// 使用指定数据库文件执行密钥提取测试
async fn test_key_extraction_with_db(
    process: &wechat_key_extractor::process::WeChatProcess, 
    db_path: &Path
) -> Result<()> {
    println!("\n🔐 开始密钥提取...");
    
    // 验证数据库文件
    if !db_path.exists() {
        return Err(Error::FileNotFound(db_path.display().to_string()));
    }
    
    let file_size = db_path.metadata()?.len();
    println!("📊 数据库文件大小: {} 字节", file_size);
    
    if file_size < 4096 {
        warn!("数据库文件可能太小");
    }
    
    // 创建验证器
    println!("🔍 创建密钥验证器...");
    let validator = Validator::from_database_file(db_path, process.version)?;
    
    // 创建密钥提取器
    let mut extractor = KeyExtractor::new();
    extractor.set_validator(validator);
    
    // 执行密钥提取
    println!("⚡ 从进程内存提取密钥...");
    let start_time = std::time::Instant::now();
    
    match extractor.extract_keys(process).await {
        Ok(keys) => {
            let duration = start_time.elapsed();
            println!("✅ 密钥提取完成，耗时: {:?}", duration);
            
            print_extraction_results(&keys, process.version);
            
            // 验证提取的密钥
            validate_extracted_keys(&keys, process.version)?;
            
            Ok(())
        }
        Err(e) => {
            let duration = start_time.elapsed();
            eprintln!("❌ 密钥提取失败，耗时: {:?}", duration);
            Err(e)
        }
    }
}

/// 打印提取结果
fn print_extraction_results(keys: &wechat_key_extractor::extractor::ExtractedKeys, version: u32) {
    println!("\n📋 提取结果:");
    println!("  微信版本: V{}", version);
    
    if let Some(ref data_key) = keys.data_key {
        println!("  📊 数据密钥: {}", data_key);
        println!("      长度: {} 字节", hex::decode(data_key).map(|v| v.len()).unwrap_or(0));
    } else {
        println!("  📊 数据密钥: ❌ 未找到");
    }
    
    if version == version::V4 {
        if let Some(ref image_key) = keys.image_key {
            println!("  🖼️  图片密钥: {}", image_key);
            println!("      长度: {} 字节", hex::decode(image_key).map(|v| v.len()).unwrap_or(0));
        } else {
            println!("  🖼️  图片密钥: ❌ 未找到");
        }
    }
    
    if keys.is_complete(version) {
        println!("  ✅ 所有必需的密钥都已找到");
    } else {
        println!("  ⚠️  部分密钥缺失");
    }
}

/// 验证提取的密钥
fn validate_extracted_keys(keys: &wechat_key_extractor::extractor::ExtractedKeys, version: u32) -> Result<()> {
    println!("\n🔍 验证提取的密钥...");
    
    // 验证数据密钥
    if let Some(ref data_key) = keys.data_key {
        if let Ok(key_bytes) = hex::decode(data_key) {
            if key_bytes.len() == 32 {
                println!("  ✅ 数据密钥长度正确 (32字节)");
                
                // 检查密钥是否全为零（通常表示无效）
                if key_bytes.iter().all(|&b| b == 0) {
                    warn!("  ⚠️  数据密钥全为零，可能无效");
                } else {
                    println!("  ✅ 数据密钥包含有效数据");
                }
            } else {
                error!("  ❌ 数据密钥长度异常: {} 字节 (期望32字节)", key_bytes.len());
                return Err(Error::InvalidKeyLength);
            }
        } else {
            error!("  ❌ 数据密钥格式无效 (非十六进制)");
            return Err(Error::InvalidKeyFormat);
        }
    } else {
        error!("  ❌ 未找到数据密钥");
        return Err(Error::NoValidKey);
    }
    
    // 验证图片密钥（仅V4版本）
    if version == version::V4 {
        if let Some(ref image_key) = keys.image_key {
            if let Ok(key_bytes) = hex::decode(image_key) {
                if key_bytes.len() == 16 {
                    println!("  ✅ 图片密钥长度正确 (16字节)");
                    
                    if key_bytes.iter().all(|&b| b == 0) {
                        warn!("  ⚠️  图片密钥全为零，可能无效");
                    } else {
                        println!("  ✅ 图片密钥包含有效数据");
                    }
                } else {
                    error!("  ❌ 图片密钥长度异常: {} 字节 (期望16字节)", key_bytes.len());
                    return Err(Error::InvalidKeyLength);
                }
            } else {
                error!("  ❌ 图片密钥格式无效 (非十六进制)");
                return Err(Error::InvalidKeyFormat);
            }
        } else {
            warn!("  ⚠️  V4版本未找到图片密钥");
        }
    }
    
    println!("  ✅ 密钥验证通过");
    Ok(())
}

/// 打印故障排除提示
fn print_troubleshooting_tips(error: &Error) {
    println!("\n🔧 故障排除提示:");
    
    match error {
        Error::SIPEnabled => {
            println!("  📱 macOS SIP问题:");
            println!("    1. 重启Mac并按住Cmd+R进入恢复模式");
            println!("    2. 打开终端，运行: csrutil disable");
            println!("    3. 重启Mac，测试完成后可用csrutil enable重新启用");
        }
        Error::ProcessOpenFailed(_) => {
            println!("  🔐 进程访问权限问题:");
            if cfg!(windows) {
                println!("    - 尝试以管理员身份运行");
                println!("    - 检查防病毒软件是否阻止");
            } else {
                println!("    - 检查是否有足够的权限访问进程");
                println!("    - 在macOS上确保SIP已禁用");
            }
        }
        Error::MemoryReadFailed(_) => {
            println!("  💾 内存读取问题:");
            println!("    - 确保微信正在运行且已完全加载");
            println!("    - 尝试重启微信");
            println!("    - 检查微信版本是否受支持");
        }
        Error::NoValidKey => {
            println!("  🔑 未找到有效密钥:");
            println!("    - 确保微信已登录且数据库文件正确");
            println!("    - 尝试发送几条消息后重试");
            println!("    - 检查微信版本是否匹配");
        }
        Error::FileNotFound(path) => {
            println!("  📁 文件未找到: {}", path);
            println!("    - 检查文件路径是否正确");
            println!("    - 确保微信已创建数据库文件");
        }
        _ => {
            println!("  🔍 通用建议:");
            println!("    - 确保微信正在运行");
            println!("    - 检查权限设置");
            println!("    - 尝试重启微信和测试程序");
        }
    }
}

/// 便捷的测试运行函数
/// 
/// 使用方法：
/// ```
/// WECHAT_DB_PATH=/path/to/MSG0.db cargo test --release run_live_key_test -- --nocapture --ignored
/// ```
#[tokio::test]
#[ignore]
async fn run_live_key_test() {
    env_logger::try_init().ok();
    test_real_wechat_key_extraction().await;
}
