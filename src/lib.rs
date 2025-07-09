use std::io::{self, Write, BufWriter};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::Duration;
use chrono::Local;

const MASTER_VERSION: &str = "0.1.0";
const BUILD_VERSION: &str = "win0";

/// UDP 消息处理器
pub struct UdpMessageHandler {
    sender_socket: UdpSocket,
    receiver_thread: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    output_file: PathBuf,
    receive_port: Option<u16>,
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
        })
    }
    
    /// 启动消息接收器
    pub fn start_receiver(&mut self, port: u16) -> io::Result<()> {
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
                    eprintln!("无法打开文件 {}: {}", output_file.display(), e);
                    None
                }
            };
            
            println!("接收器已启动，监听端口 {}", port);
            
            while running.load(Ordering::SeqCst) {
                match receiver_socket.recv_from(&mut buf) {
                    Ok((size, source)) => {
                        let message = match String::from_utf8(buf[..size].to_vec()) {
                            Ok(m) => m,
                            Err(e) => {
                                // 保存原始字节数据
                                format!("<BINARY DATA: {} bytes>", size)
                            }
                        };
                        
                        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                        let log_entry = format!(
                            "[{}] FROM {}: {}\n",
                            timestamp, source, message
                        );
                        
                        // 打印到控制台
                        println!("[UDP RECV] {}", log_entry.trim());
                        
                        // 写入文件
                        if let Some(writer) = &mut file_writer {
                            if let Err(e) = writer.write_all(log_entry.as_bytes()) {
                                eprintln!("文件写入错误: {}", e);
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
                        eprintln!("接收错误: {}", e);
                    }
                }
            }
            
            // 确保所有缓冲数据写入文件
            if let Some(mut writer) = file_writer {
                let _ = writer.flush();
            }
            
            println!("接收器已停止");
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
        match command {
            "send" => self.handle_send(handler),
            "start" => self.handle_start(handler),
            "stop" => self.handle_stop(handler),
            "status" => self.handle_status(handler),
            "version" => self.handle_version(),
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
                if let Err(e) = handler.start_receiver(port_num) {
                    eprintln!("启动接收器失败: {}", e);
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
    }

    fn handle_version(&self) {
        println!("NChat version {} {}",MASTER_VERSION,BUILD_VERSION);
    }

    /// 显示帮助信息
    pub fn show_help(&self) {
        println!("\n可用命令:");
        println!("  send   - 发送消息到指定地址");
        println!("  start  - 启动消息接收器");
        println!("  stop   - 停止消息接收器");
        println!("  status - 显示当前状态");
        println!("  version - 显示当前版本");
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