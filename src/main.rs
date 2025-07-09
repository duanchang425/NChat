use std::io::{self, Write};
use NChat::{InputHandler,UdpMessageHandler};

// mod newchat {
//     pub use crate::*;
// }

// 默认输出文件
const DEFAULT_OUTPUT_FILE: &str = "received_messages.log";

fn main() -> io::Result<()> {
    println!("UDP 消息收发程序");
    println!("消息将保存到: {}", DEFAULT_OUTPUT_FILE);

    // 添加 Windows 特定提示
    if cfg!(windows) {
        println!("提示: 在 Windows 上运行时，请确保防火墙允许 UDP 流量");
    }

    // 创建消息处理器
    let mut handler = UdpMessageHandler::new(DEFAULT_OUTPUT_FILE)?;
    let input_handler = InputHandler;
    
    // 显示初始状态
    println!("发送端口: {}", handler.local_send_port()?);
    input_handler.show_help();

    loop {
        // 显示提示符
        print!("> ");
        io::stdout().flush()?;

        // 读取用户命令
        let mut command = String::new();
        io::stdin().read_line(&mut command)?;
        let command = command.trim();
        
        // 处理命令
        if input_handler.handle_command(command, &mut handler) {
            break;
        }
    }
    
    Ok(())
}