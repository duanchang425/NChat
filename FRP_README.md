# NChat Frp 内网穿透功能使用指南

## 概述

NChat 现在集成了 frp 内网穿透功能，允许你的 NChat 服务通过公网 frp 服务器暴露给外网用户。

## 前置条件

1. **Frp 服务器**: 你需要一个运行 frp 服务器的公网服务器
2. **Frp 客户端**: 程序会自动下载或你可以手动下载 frp 客户端

## 快速开始

### 1. 配置 Frp 服务器信息

```bash
# 配置 frp 服务器
frp config your-frp-server.com 7000 your-token
```

### 2. 初始化 Frp 管理器

```bash
frp init
```

### 3. 启动内网穿透

```bash
frp start
```

### 4. 检查状态

```bash
frp status
```

## 详细命令说明

### Frp 配置命令

- `frp config <服务器地址> <端口> [token]` - 配置 frp 服务器
- `frp init` - 初始化 frp 管理器
- `frp start` - 启动内网穿透
- `frp stop` - 停止内网穿透
- `frp status` - 显示 frp 状态
- `frp download` - 下载 frp 客户端

### 示例配置

```bash
# 配置服务器
frp config frp.example.com 7000 my-secret-token

# 初始化
frp init

# 启动穿透
frp start

# 检查状态
frp status
```

## Frp 服务器配置

你的 frp 服务器需要以下配置（frps.ini）：

```ini
[common]
bind_port = 7000
token = your-secret-token

# 可选：Web 管理界面
dashboard_port = 7500
dashboard_user = admin
dashboard_pwd = admin
```

## 故障排除

### 1. Frp 客户端未找到

如果程序提示找不到 frp 客户端：

1. 手动下载 frp 客户端：
   - 访问 https://github.com/fatedier/frp/releases
   - 下载对应系统的版本
   - 将 `frpc` 或 `frpc.exe` 放在程序目录下

2. 或者使用程序自动下载功能：
   ```bash
   frp download
   ```

### 2. 连接失败

- 检查服务器地址和端口是否正确
- 确认 frp 服务器正在运行
- 检查防火墙设置
- 验证认证令牌是否正确

### 3. 端口冲突

如果遇到端口冲突：

1. 修改 frp 配置中的端口
2. 或者使用不同的代理名称

## 高级配置

### 自定义配置文件

你可以创建自定义的 frp 配置文件：

1. 复制 `frp_config_example.toml` 为 `frpc.toml`
2. 修改配置参数
3. 将配置文件放在 `frp_config/` 目录下

### 多个代理配置

你可以在配置文件中添加多个代理：

```toml
[[proxies]]
name = "nchat-tcp"
type = "tcp"
localPort = 8080

[[proxies]]
name = "nchat-udp"
type = "udp"
localPort = 8080
remotePort = 8081
```

## 安全注意事项

1. **使用强密码**: 为 frp 服务器设置强密码
2. **限制访问**: 配置防火墙只允许必要的端口
3. **定期更新**: 保持 frp 版本更新
4. **监控日志**: 定期检查 frp 服务器日志

## 技术支持

如果遇到问题：

1. 检查 frp 官方文档：https://gofrp.org/docs/
2. 查看程序日志输出
3. 确认网络连接正常
4. 验证服务器配置正确 