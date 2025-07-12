use std::process::{Child, Command, Stdio};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

/// Frp 配置结构
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FrpConfig {
    pub server_addr: String,
    pub server_port: u16,
    pub token: Option<String>,
    pub local_port: u16,
    pub remote_port: Option<u16>,
    pub protocol: String,
    pub name: String,
}

/// Frp 客户端管理器
pub struct FrpManager {
    config: FrpConfig,
    process: Arc<Mutex<Option<Child>>>,
    config_path: PathBuf,
    frp_path: Option<PathBuf>,
}

impl FrpManager {
    /// 创建新的 Frp 管理器
    pub fn new(config: FrpConfig) -> Result<Self> {
        let config_dir = PathBuf::from("frp_config");
        fs::create_dir_all(&config_dir)?;
        
        let config_path = config_dir.join("frpc.toml");
        
        Ok(Self {
            config,
            process: Arc::new(Mutex::new(None)),
            config_path,
            frp_path: None,
        })
    }
    
    /// 设置 frp 客户端路径
    pub fn set_frp_path(&mut self, path: PathBuf) {
        self.frp_path = Some(path);
    }
    
    /// 生成 frp 配置文件
    pub fn generate_config(&self) -> Result<()> {
        let mut config_content = String::new();
        
        // 服务器配置
        config_content.push_str(&format!("serverAddr = \"{}\"\n", self.config.server_addr));
        config_content.push_str(&format!("serverPort = {}\n", self.config.server_port));
        
        if let Some(ref token) = self.config.token {
            config_content.push_str(&format!("auth.token = \"{}\"\n", token));
        }
        
        // 代理配置
        config_content.push_str("\n[[proxies]]\n");
        config_content.push_str(&format!("name = \"{}\"\n", self.config.name));
        config_content.push_str(&format!("type = \"{}\"\n", self.config.protocol));
        config_content.push_str(&format!("localPort = {}\n", self.config.local_port));
        
        if let Some(remote_port) = self.config.remote_port {
            config_content.push_str(&format!("remotePort = {}\n", remote_port));
        }
        
        // 写入配置文件
        let mut file = File::create(&self.config_path)
            .context("创建 frp 配置文件失败")?;
        file.write_all(config_content.as_bytes())
            .context("写入 frp 配置文件失败")?;
        
        println!("Frp 配置文件已生成: {}", self.config_path.display());
        Ok(())
    }
    
    /// 启动 frp 客户端
    pub fn start(&mut self) -> Result<()> {
        if self.is_running() {
            return Err(anyhow::anyhow!("Frp 客户端已在运行"));
        }
    
        // 生成配置文件
        self.generate_config()?;
    
        // 确定 frp 客户端路径
        let frp_path = if let Some(ref path) = self.frp_path {
            path.clone()
        } else {
            // 获取当前工作目录
            let current_dir = std::env::current_dir()
                .context("获取当前工作目录失败")?;
            
            let frpc_path = if cfg!(windows) {
                current_dir.join("frpc.exe")
            } else {
                current_dir.join("frpc")
            };
            
            if frpc_path.exists() {
                frpc_path
            } else {
                return Err(anyhow::anyhow!(
                    "未找到 frp 客户端: {}\n请下载 frp 并将 frpc 放在当前目录，或使用 set_frp_path 设置路径",
                    frpc_path.display()
                ));
            }
        };
    
        // 打印调试信息
        println!("正在启动 frp 客户端...");
        println!("可执行文件路径: {}", frp_path.display());
        println!("配置文件路径: {}", self.config_path.display());
    
        // 检查文件是否存在
        if !frp_path.exists() {
            return Err(anyhow::anyhow!("frp 可执行文件不存在: {}", frp_path.display()));
        }
        if !self.config_path.exists() {
            return Err(anyhow::anyhow!("配置文件不存在: {}", self.config_path.display()));
        }
    
                // 构造命令
        let mut command = Command::new(&frp_path);
        command.arg("-c");
        command.arg(&self.config_path);

        // 重定向输出到文件，避免干扰主程序交互
        let log_file = std::env::current_dir()
            .context("获取当前工作目录失败")?
            .join("frpc.log");
        
        let stdout_file = File::create(&log_file)
            .context("创建 frpc 日志文件失败")?;
        let stderr_file = File::create(&log_file)
            .context("创建 frpc 日志文件失败")?;
        
        command.stdout(Stdio::from(stdout_file));
        command.stderr(Stdio::from(stderr_file));
    
        // 启动进程
        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                println!("调试信息: 尝试启动命令: {:?}", command);
                println!("系统错误: {:?}", e);
                println!("错误类型: {:?}", e.kind());
                if let Some(code) = e.raw_os_error() {
                    println!("系统错误码: {}", code);
                }
                return Err(anyhow::anyhow!(
                    "启动 frp 客户端失败: {}\n请检查：\n1. 文件 {} 是否存在且可执行\n2. 配置文件 {} 是否存在且格式正确\n3. 是否有足够的权限运行程序",
                    e,
                    frp_path.display(),
                    self.config_path.display()
                ));
            }
        };
    
        println!("Frp 客户端已启动 (PID: {})", child.id());
        println!(
            "本地端口 {} 将通过 frp 服务器 {}:{} 暴露",
            self.config.local_port, self.config.server_addr, self.config.server_port
        );
    
        // 保存进程句柄
        {
            let mut process_guard = self.process.lock().unwrap();
            *process_guard = Some(child);
        }
    
        Ok(())
    }
    
    /// 停止 frp 客户端
    pub fn stop(&mut self) -> Result<()> {
        let mut process_guard = self.process.lock().unwrap();
        
        if let Some(mut child) = process_guard.take() {
            // 尝试优雅地终止进程
            if let Err(e) = child.kill() {
                eprintln!("终止 frp 进程失败: {}", e);
            } else {
                // 等待进程结束
                let _ = child.wait();
                println!("Frp 客户端已停止");
            }
        }
        
        Ok(())
    }
    
    /// 检查 frp 客户端是否正在运行
    pub fn is_running(&self) -> bool {
        let process_guard = self.process.lock().unwrap();
        process_guard.is_some()
    }
    
    /// 获取 frp 状态信息
    pub fn get_status(&self) -> FrpStatus {
        let process_guard = self.process.lock().unwrap();
        
        FrpStatus {
            is_running: process_guard.is_some(),
            config: self.config.clone(),
            config_path: self.config_path.clone(),
        }
    }
    
    /// 下载 frp 客户端（如果不存在）
    pub async fn download_frp_if_needed(&mut self) -> Result<()> {
        let frpc_name = if cfg!(windows) { "frpc.exe" } else { "frpc" };
        let frpc_path = PathBuf::from(frpc_name);
        
        if frpc_path.exists() {
            println!("Frp 客户端已存在: {}", frpc_path.display());
            return Ok(());
        }
        
        println!("正在下载 frp 客户端...");
        
        // 根据系统架构确定下载 URL
        let (url, filename) = if cfg!(windows) {
            if cfg!(target_arch = "x86_64") {
                ("https://github.com/fatedier/frp/releases/download/v0.51.3/frp_0.51.3_windows_amd64.zip", "frp_windows_amd64.zip")
            } else {
                ("https://github.com/fatedier/frp/releases/download/v0.51.3/frp_0.51.3_windows_386.zip", "frp_windows_386.zip")
            }
        } else {
            if cfg!(target_arch = "x86_64") {
                ("https://github.com/fatedier/frp/releases/download/v0.51.3/frp_0.51.3_linux_amd64.tar.gz", "frp_linux_amd64.tar.gz")
            } else {
                ("https://github.com/fatedier/frp/releases/download/v0.51.3/frp_0.51.3_linux_386.tar.gz", "frp_linux_386.tar.gz")
            }
        };
        
        // 下载文件
        let response = reqwest::get(url).await
            .context("下载 frp 失败")?;
        
        let bytes = response.bytes().await
            .context("读取下载内容失败")?;
        
        fs::write(&filename, &bytes)
            .context("保存下载文件失败")?;
        
        // 解压文件
        if cfg!(windows) {
            // Windows 使用 zip
            let file = File::open(&filename)
                .context("打开下载文件失败")?;
            let mut archive = zip::ZipArchive::new(file)
                .context("读取 zip 文件失败")?;
            
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)
                    .context("访问 zip 文件条目失败")?;
                
                if file.name().ends_with("frpc.exe") {
                    let mut outfile = File::create(&frpc_name)
                        .context("创建 frpc.exe 失败")?;
                    std::io::copy(&mut file, &mut outfile)
                        .context("解压 frpc.exe 失败")?;
                    break;
                }
            }
        } else {
            // Linux 使用 tar.gz
            let file = File::open(&filename)
                .context("打开下载文件失败")?;
            let gz = flate2::read::GzDecoder::new(file);
            let mut tar = tar::Archive::new(gz);
            
            for entry in tar.entries()
                .context("读取 tar 文件失败")? {
                let mut entry = entry.context("访问 tar 条目失败")?;
                
                if entry.path().unwrap().to_str().unwrap().ends_with("frpc") {
                    let mut outfile = File::create(&frpc_name)
                        .context("创建 frpc 失败")?;
                    std::io::copy(&mut entry, &mut outfile)
                        .context("解压 frpc 失败")?;
                    
                    // 设置执行权限（仅在 Unix 系统上）
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&frpc_name, fs::Permissions::from_mode(0o755))
                            .context("设置执行权限失败")?;
                    }
                    break;
                }
            }
        }
        
        // 清理下载文件
        let _ = fs::remove_file(&filename);
        
        println!("Frp 客户端下载完成: {}", frpc_path.display());
        self.frp_path = Some(frpc_path);
        
        Ok(())
    }
}

/// Frp 状态信息
#[derive(Debug, Clone)]
pub struct FrpStatus {
    pub is_running: bool,
    pub config: FrpConfig,
    pub config_path: PathBuf,
}

impl Drop for FrpManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// 默认 frp 配置
pub fn default_frp_config() -> FrpConfig {
    FrpConfig {
        server_addr: "frp.example.com".to_string(),
        server_port: 7000,
        token: None,
        local_port: 7000,
        remote_port: None,
        protocol: "tcp".to_string(),
        name: "nchat".to_string(),
    }
} 