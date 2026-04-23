# GPUpass

**[English](#english) | [中文](#chinese)**

---

<a name="english"></a>
## English

### Description

GPUpass is a Terminal User Interface (TUI) tool written in Rust for managing GPU passthrough to virtual machines. It provides an intuitive terminal-based interface to configure and manage GPU device assignment for VMs.

### Features

- TUI-based interactive interface for GPU passthrough management
- GPU device detection and listing
- Virtual machine configuration management
- Passthrough setup and management
- Multi-language support

### SCREENSHOT
<img width="1920" height="1200" alt="Screenshot_23-4月_19-37-36_12932" src="https://github.com/user-attachments/assets/923b81ec-b647-4157-a154-7571951771ee" />
<img width="1920" height="1200" alt="Screenshot_23-4月_19-39-03_27846" src="https://github.com/user-attachments/assets/92700b2b-1119-48b5-81cc-8d64ca853099" />


### Prerequisites

- Rust toolchain (edition 2021 or later)
- Linux operating system
- Appropriate permissions for GPU passthrough operations

### Installation

```bash
# Clone the repository
git clone https://github.com/yangstafiltra/GPUpass.git
cd GPUpass

# Build the project
cargo build --release

# Run
sudo ./target/release/gpupass
```

### Dependencies

- **ratatui** - Terminal UI framework
- **crossterm** - Terminal manipulation library
- **serde / serde_json** - Serialization and deserialization
- **which** - Locate executable binaries
- **libc** - Raw FFI bindings
- **tempfile** - Temporary file handling

### License

MIT

---

<a name="chinese"></a>
## 中文

### 项目简介

GPUpass 是一个使用 Rust 编写的终端用户界面（TUI）工具，用于管理虚拟机中的 GPU 直通功能。它提供了一个直观的终端界面，用于配置和管理虚拟机 GPU 设备分配。

### 功能特性

- 基于 TUI 的交互式 GPU 直通管理界面
- GPU 设备检测与列表
- 虚拟机配置管理
- 直通设置与管理
- 多语言支持

### 截图
<img width="1920" height="1200" alt="Screenshot_23-4月_19-37-36_12932" src="https://github.com/user-attachments/assets/f5279d64-95dc-49fe-8750-919ed752a4b4" />
<img width="1920" height="1200" alt="Screenshot_23-4月_19-34-58_735" src="https://github.com/user-attachments/assets/bbdaa0e0-25d0-435b-8619-0c5f5b682cd8" />


### 前置要求

- Rust 工具链（edition 2021 或更高版本）
- Linux 操作系统
- GPU 直通操作所需的适当权限

### 安装方法

```bash
# 克隆仓库
git clone https://github.com/yangstafiltra/GPUpass.git
cd GPUpass

# 构建项目
cargo build --release

# 运行
sudo ./target/release/gpupass
```

### 依赖项

- **ratatui** - 终端 UI 框架
- **crossterm** - 终端操作库
- **serde / serde_json** - 序列化与反序列化
- **which** - 定位可执行二进制文件
- **libc** - 原始 FFI 绑定
- **tempfile** - 临时文件处理

### 许可证

MIT

---

## AI-Generated Notice / AI 生成声明

**EN:** This project was developed with the assistance of Artificial Intelligence (AI). While every effort has been made to ensure code quality and correctness, AI-generated content may contain errors or suboptimal implementations. If you encounter any issues, bugs, or have suggestions for improvement, please don't hesitate to [open an issue](https://github.com/yangstafiltra/GPUpass/issues) or submit a pull request. Your feedback is highly valuable and greatly contributes to the ongoing improvement of this project.

**中:** 本项目是在人工智能（AI）的协助下开发的。尽管我们已尽力确保代码质量和正确性，但 AI 生成的内容可能存在错误或非最优实现。如果您遇到任何问题、Bug 或有改进建议，请随时[提交 Issue](https://github.com/yangstafiltra/GPUpass/issues) 或提交 Pull Request。您的反馈非常宝贵，对本项目的持续改进至关重要。
