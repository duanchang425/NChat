use std::io::{self, Write, BufWriter};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::Duration;
use chrono::Local;
use std::sync::mpsc::{Sender, Receiver, channel};

// 添加 frp 模块
pub mod frp;
use frp::{FrpManager, FrpConfig, default_frp_config};

const MASTER_VERSION: &str = "1.0.1";
const BUILD_VERSION: &str = "win0";

/// UDP 消息处理器
pub struct UdpMessageHandler {
    sender_socket: UdpSocket,
    receiver_thread: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    output_file: PathBuf,
    receive_port: Option<u16>,
    frp_manager: Option<FrpManager>, // 添加 frp 管理器
    status_sender: Option<Sender<String>>, // 新增
}

impl UdpMessageHandler {
    /// 创建新的消息处理器
    pub fn new(output_file: &str) -> io::Result<Self> {
        // 创建发送套接字（绑定随机端口）
        let sender_socket = UdpSocket::bind("0.0.0.0:0")?;
        sender_socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        
        // 确保输出目录存在
        let output_path = PathBuf::from(output_file);
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // 创建或打开输出文件
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_path)?;
        
        Ok(Self {
            sender_socket,
            receiver_thread: None,
            running: Arc::new(AtomicBool::new(false)),
            output_file: output_path,
            receive_port: None,
            frp_manager: None,
            status_sender: None, // 新增
        })
    }
    
    /// 启动消息接收器
    pub fn start_receiver(&mut self, port: u16, status_sender: Sender<String>) -> io::Result<()> {
        if self.is_receiving() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "接收器已在运行",
            ));
        }
        
        // 创建接收套接字
        let receiver_socket = UdpSocket::bind(("0.0.0.0", port))?;
        receiver_socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        
        // 设置运行标志
        self.running.store(true, Ordering::SeqCst);
        self.receive_port = Some(port);
        self.status_sender = Some(status_sender.clone());
        
        // 克隆共享状态
        let running = self.running.clone();
        let output_file = self.output_file.clone();
        
        // 启动接收线程
        let handle = thread::spawn(move || {
            let mut buf = [0; 2048]; // 更大的缓冲区处理长消息
            
            // 打开文件用于追加写入
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&output_file);
            
            let mut file_writer = match file {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    let _ = status_sender.send(format!("无法打开文件 {}: {}", output_file.display(), e));
                    None
                }
            };
            
            // 启动信息通过通道发送
            let _ = status_sender.send(format!("接收器已启动，监听端口 {}", port));
            
            while running.load(Ordering::SeqCst) {
                match receiver_socket.recv_from(&mut buf) {
                    Ok((size, source)) => {
                        let message = match String::from_utf8(buf[..size].to_vec()) {
                            Ok(m) => m,
                            Err(_) => format!("<BINARY DATA: {} bytes>", size)
                        };
                        
                        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                        let log_entry = format!(
                            "[{}] FROM {}: {}\n",
                            timestamp, source, message
                        );
                        
                        // 只写入文件，不输出到控制台
                        if let Some(writer) = &mut file_writer {
                            if let Err(e) = writer.write_all(log_entry.as_bytes()) {
                                let _ = status_sender.send(format!("文件写入错误: {}", e));
                            }
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock ||
                    e.kind() == io::ErrorKind::TimedOut => {
                        continue;
                    }
                        // 添加更详细的错误信息
                        // if e.kind() == io::ErrorKind::WouldBlock {
                        //     // 正常超时，无需报告
                        // } else if cfg!(windows) && e.raw_os_error() == Some(10060) {
                        //     // Windows 特定错误处理
                        //     eprintln!("接收超时 (Windows 错误 10060) - 可能是防火墙问题或网络配置问题");
                        // } else {
                        //     eprintln!("接收错误 [{}]: {}", e.kind(), e);
                        // }
                        
                    
                    Err(e) => {
                        let _ = status_sender.send(format!("接收错误: {}", e));
                    }
                }
            }
            
            // 确保所有缓冲数据写入文件
            if let Some(mut writer) = file_writer {
                let _ = writer.flush();
            }
            
            // 停止信息通过通道发送
            let _ = status_sender.send("接收器已停止".to_string());
        });
        
        self.receiver_thread = Some(handle);
        Ok(())
    }
    
    /// 停止消息接收器
    pub fn stop_receiver(&mut self) {
        if self.is_receiving() {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.receiver_thread.take() {
                let _ = handle.join();
            }
            self.receive_port = None;
        }
    }
    
    /// 检查接收器是否正在运行
    pub fn is_receiving(&self) -> bool {
        self.receiver_thread.is_some()
    }
    
    /// 发送消息到指定地址
    // pub fn send_message(&self, target: &str, message: &str) -> io::Result<usize> {
    //     let addr: SocketAddr = target.parse().map_err(|e| {
    //         io::Error::new(
    //             io::ErrorKind::InvalidInput, 
    //             format!("无效的目标地址: {}", e)
    //         )
    //     })?;
        
    //     self.sender_socket.send_to(message.as_bytes(), addr)
    // }
    pub fn send_message(&self, target: &str, message: &str) -> io::Result<usize> {
        // 解析目标地址
        let addr: SocketAddr = match target.parse() {
            Ok(addr) => addr,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput, 
                    format!("无效的目标地址格式: {} - {}", target, e)
                ))
            }
        };

        // 设置发送超时 (Windows 特定修复)
        self.sender_socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        
        // 发送消息
        match self.sender_socket.send_to(message.as_bytes(), &addr) {
            Ok(size) => Ok(size),
            Err(e) => {
                // 处理 Windows 特有的错误报告问题
                if cfg!(windows) && e.kind() == io::ErrorKind::TimedOut {
                    Err(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        format!("无法发送到 {}: 目标可能不可达或未响应", addr)
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }
    
    /// 获取发送器绑定的本地端口
    pub fn local_send_port(&self) -> io::Result<u16> {
        self.sender_socket.local_addr().map(|addr| addr.port())
    }
    
    /// 获取接收端口
    pub fn receive_port(&self) -> Option<u16> {
        self.receive_port
    }
    
    /// 获取输出文件路径
    pub fn output_file(&self) -> &PathBuf {
        &self.output_file
    }
    
    // ========== Frp 相关方法 ==========
    
    /// 初始化 frp 管理器
    pub fn init_frp(&mut self, config: Option<FrpConfig>) -> anyhow::Result<()> {
        let config = config.unwrap_or_else(default_frp_config);
        let mut frp_manager = FrpManager::new(config)?;
        
        // 设置本地端口为当前接收端口
        if let Some(port) = self.receive_port {
            // 这里需要修改 frp 配置的本地端口
            // 由于 FrpConfig 是值类型，我们需要重新创建
            let mut new_config = frp_manager.get_status().config;
            new_config.local_port = port;
            frp_manager = FrpManager::new(new_config)?;
        }
        
        self.frp_manager = Some(frp_manager);
        println!("Frp 管理器已初始化");
        Ok(())
    }
    
    /// 启动 frp 内网穿透
    pub fn start_frp(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut frp_manager) = self.frp_manager {
            frp_manager.start()?;
            println!("Frp 内网穿透已启动");
            Ok(())
        } else {
            Err(anyhow::anyhow!("请先初始化 frp 管理器"))
        }
    }
    
    /// 停止 frp 内网穿透
    pub fn stop_frp(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut frp_manager) = self.frp_manager {
            frp_manager.stop()?;
            println!("Frp 内网穿透已停止");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Frp 管理器未初始化"))
        }
    }
    
    /// 检查 frp 是否正在运行
    pub fn is_frp_running(&self) -> bool {
        self.frp_manager.as_ref().map(|m| m.is_running()).unwrap_or(false)
    }
    
    /// 获取 frp 状态
    pub fn get_frp_status(&self) -> Option<frp::FrpStatus> {
        self.frp_manager.as_ref().map(|m| m.get_status())
    }
    
    /// 下载 frp 客户端
    pub async fn download_frp(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut frp_manager) = self.frp_manager {
            frp_manager.download_frp_if_needed().await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("请先初始化 frp 管理器"))
        }
    }
    
    /// 配置 frp 服务器
    pub fn configure_frp(&mut self, server_addr: &str, server_port: u16, token: Option<&str>) -> anyhow::Result<()> {
        let config = FrpConfig {
            server_addr: server_addr.to_string(),
            server_port,
            token: token.map(|t| t.to_string()),
            local_port: self.receive_port.unwrap_or(8080),
            remote_port: None,
            protocol: "tcp".to_string(),
            name: "nchat".to_string(),
        };
        
        self.frp_manager = Some(FrpManager::new(config)?);
        println!("Frp 配置已更新");
        Ok(())
    }
}

impl Drop for UdpMessageHandler {
    fn drop(&mut self) {
        self.stop_receiver();
    }
}

/// 用户输入处理器
pub struct InputHandler;

impl InputHandler {
    /// 处理用户命令
    pub fn handle_command(
        &self, 
        command: &str, 
        handler: &mut UdpMessageHandler,
    ) -> bool {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.get(0).unwrap_or(&"");
        
        match *cmd {
            "send" => self.handle_send(handler),
            "start" => self.handle_start(handler),
            "stop" => self.handle_stop(handler),
            "status" => self.handle_status(handler),
            "version" => self.handle_version(),
            "frp" => {
                self.handle_frp(handler, &parts[1..]);
            }
            "quit" => {
                println!("程序退出");
                return true;
            }
            "" => {} // 忽略空输入
            "help" => self.show_help(),
            _ => println!("未知命令 '{}'，输入 'help' 查看帮助", command),
        }
        false
    }

    /// 处理发送命令
    fn handle_send(&self, handler: &UdpMessageHandler) {
        // 获取目标地址
        let target = match self.prompt_input("请输入目标地址 (格式: IP:端口, 例如 127.0.0.1:8080)") {
            Some(addr) => addr,
            None => return,
        };

        // 获取要发送的消息
        let message = match self.prompt_input("请输入要发送的消息") {
            Some(msg) => msg,
            None => return,
        };

        // 发送消息
        match handler.send_message(&target, &message) {
            Ok(size) => println!("成功发送 {} 字节到 {}", size, target),
            Err(e) => eprintln!("发送失败: {}", e),
        }
    }
    
    /// 处理启动接收命令
    fn handle_start(&self, handler: &mut UdpMessageHandler) {
        if handler.is_receiving() {
            println!("接收器已在运行");
            return;
        }
        
        let port = match self.prompt_input("请输入接收端口 (例如 8080)") {
            Some(p) => p,
            None => return,
        };
        
        match port.parse::<u16>() {
            Ok(port_num) => {
                let (status_tx, status_rx) = channel::<String>();
                if let Err(e) = handler.start_receiver(port_num, status_tx) {
                    eprintln!("启动接收器失败: {}", e);
                } else {
                    // 启动一个线程来监听状态通道
                    thread::spawn(move || {
                        while let Ok(msg) = status_rx.try_recv() {
                            println!("{}", msg);
                        }
                    });
                }
            }
            Err(_) => eprintln!("无效的端口号"),
        }
    }
    
    /// 处理停止接收命令
    fn handle_stop(&self, handler: &mut UdpMessageHandler) {
        if !handler.is_receiving() {
            println!("接收器未运行");
            return;
        }
        handler.stop_receiver();
        println!("接收器已停止");
    }
    
    /// 处理状态命令
    fn handle_status(&self, handler: &UdpMessageHandler) {
        println!("=== NChat 状态 ===");
        match handler.local_send_port() {
            Ok(port) => println!("发送端口: {}", port),
            Err(e) => eprintln!("获取发送端口失败: {}", e),
        }
        
        println!("接收器状态: {}", 
            if handler.is_receiving() { "运行中" } else { "已停止" });
        
        if let Some(port) = handler.receive_port() {
            println!("接收端口: {}", port);
        }
        
        println!("消息保存路径: {}", handler.output_file().display());
        
        // 显示 frp 状态
        println!("\n=== Frp 内网穿透状态 ===");
        println!("运行状态: {}", if handler.is_frp_running() { "运行中" } else { "已停止" });
        
        if let Some(status) = handler.get_frp_status() {
            println!("服务器地址: {}:{}", status.config.server_addr, status.config.server_port);
            println!("本地端口: {}", status.config.local_port);
            println!("协议: {}", status.config.protocol);
            println!("代理名称: {}", status.config.name);
        } else {
            println!("Frp 未初始化");
        }
    }

    fn handle_version(&self) {
        println!("NChat version {} {}",MASTER_VERSION,BUILD_VERSION);
    }
    
    /// 处理 frp 相关命令
    fn handle_frp(&self, handler: &mut UdpMessageHandler, args: &[&str]) {
        if args.is_empty() {
            self.show_frp_help();
            return;
        }
        
        match args[0] {
            "init" => {
                if let Err(e) = handler.init_frp(None) {
                    eprintln!("初始化 frp 失败: {}", e);
                }
            }
            "start" => {
                if let Err(e) = handler.start_frp() {
                    eprintln!("启动 frp 失败: {}", e);
                }
            }
            "stop" => {
                if let Err(e) = handler.stop_frp() {
                    eprintln!("停止 frp 失败: {}", e);
                }
            }
            "status" => {
                self.handle_frp_status(handler);
            }
            "config" => {
                if args.len() < 3 {
                    println!("用法: frp config <服务器地址> <端口> [token]");
                    return;
                }
                let server_addr = args[1];
                let server_port = match args[2].parse::<u16>() {
                    Ok(port) => port,
                    Err(_) => {
                        eprintln!("无效的端口号: {}", args[2]);
                        return;
                    }
                };
                let token = args.get(3).copied();
                
                if let Err(e) = handler.configure_frp(server_addr, server_port, token) {
                    eprintln!("配置 frp 失败: {}", e);
                }
            }
            "download" => {
                println!("正在下载 frp 客户端...");
                // 由于 download_frp 是异步方法，我们需要在运行时处理
                // 这里简化处理，提示用户手动下载
                println!("请手动下载 frp 客户端并放置在当前目录");
                println!("下载地址: https://github.com/fatedier/frp/releases");
            }
            _ => {
                println!("未知的 frp 命令: {}", args[0]);
                self.show_frp_help();
            }
        }
    }
    
    /// 显示 frp 状态
    fn handle_frp_status(&self, handler: &UdpMessageHandler) {
        println!("=== Frp 状态 ===");
        println!("运行状态: {}", if handler.is_frp_running() { "运行中" } else { "已停止" });
        
        if let Some(status) = handler.get_frp_status() {
            println!("服务器地址: {}:{}", status.config.server_addr, status.config.server_port);
            println!("本地端口: {}", status.config.local_port);
            println!("协议: {}", status.config.protocol);
            println!("代理名称: {}", status.config.name);
            if let Some(ref token) = status.config.token {
                println!("认证令牌: {}", token);
            }
            println!("配置文件: {}", status.config_path.display());
        } else {
            println!("Frp 未初始化");
        }
    }
    
    /// 显示 frp 帮助信息
    fn show_frp_help(&self) {
        println!("\n=== Frp 内网穿透命令 ===");
        println!("  frp init     - 初始化 frp 管理器");
        println!("  frp config   - 配置 frp 服务器 (用法: frp config <服务器地址> <端口> [token])");
        println!("  frp start    - 启动 frp 内网穿透");
        println!("  frp stop     - 停止 frp 内网穿透");
        println!("  frp status   - 显示 frp 状态");
        println!("  frp download - 下载 frp 客户端");
        println!("\n示例:");
        println!("  frp config frp.example.com 7000 mytoken");
        println!("  frp init");
        println!("  frp start");
    }

    /// 显示帮助信息
    pub fn show_help(&self) {
        println!("\n可用命令:");
        println!("  send   - 发送消息到指定地址");
        println!("  start  - 启动消息接收器");
        println!("  stop   - 停止消息接收器");
        println!("  status - 显示当前状态");
        println!("  version - 显示当前版本");
        println!("  frp    - Frp 内网穿透管理 (输入 'frp' 查看详细帮助)");
        println!("  quit   - 退出程序");
        println!("  help   - 显示此帮助信息");
        println!("本软件遵守GPL v3协议.详见https://github.com/duanchang425/YChat")
    }

    /// 提示用户输入并读取结果
    fn prompt_input(&self, prompt: &str) -> Option<String> {
        print!("{}: ", prompt);
        if let Err(e) = io::stdout().flush() {
            eprintln!("输出错误: {}", e);
            return None;
        }

        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            eprintln!("输入错误: {}", e);
            return None;
        }

        Some(input.trim().to_string())
    }
}