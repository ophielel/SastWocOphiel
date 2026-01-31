# Ophiel（一瞬追忆）的Woc项目

## 项目概述

这是一个应该、大部分、几乎完成了所有任务的文档，题目里面让我push过来我就push过来了，别的我也不知道啊。该项目的完成借助了大量的AI帮助，特别鸣谢GPT指导，D指导，G指导（C指导太贵了用不起）。

## 已经完成的任务

### 任务1：修复Tetris模块里面的panic
- **问题**：原始代码中的`panic!("Try fix me!")导致加载失败
- **解决方案：**移除并重新编写

### 任务2：加载magic.ko,用ioctl触发flag
- **问题**：需要调用magic设备的ioctl（0x1337）获取flag
- **解决方案：**自己写一个c语言程序塞里面，在QEMU里面运行然后输出到dmesg

### 任务3：CI/CD + OCI镜像
- **GITHUB ACTION**：自动化构建测试和打包
- **Docker镜像**：提供可复现的运行环境
- **无KVM支持**：在无硬件虚拟化环境下也能运行测试

### 任务4：Tetris模块的debugfs调试接口
待完成。

## 快速开始

### 环境要求
- Linux系统（推荐Ubuntu/Debian）
- Docker（可选）
- QEMU（已包含在Docker镜像中）

### 使用Dokcer运行（推荐）

```bash
# 构建镜像
docker build -t woc2026-solution

# 运行（带KVM加速）
docker run --privileged -it woc2026-solution

# 运行（无KVM）
docker run -it woc2026-solution
```

### 本地运行

```bash
# 构建镜像
sudo apt-get install qemu-system-x86 make gcc clang lld python3 bc bison flex pkg-config cpio gzip

# 构建项目
make setup
make build(时间较长)

# 启动QEMU
make run               #使用KVM加速
make run-no-kvm        #无KVM加速


## 项目使用

### 1.运行Tetris游戏

```bash
#QEMU系统中
play_tetris
```

### 2.触发Magic模块flag

```bash
# 在QEMU系统中
magic_test

# 查看flag
dmsg | tail -10
```

### 3.使用DebugFS调试Tetris

```bash
# 挂载debugfs
mount -t debugfs none /sys/kernel/debug

# 查看Tetris状态
cat /sys/kernel/debug/tetris/status

# 查看游戏棋盘
cat /sys/kernal/deug/tetris/board

# 控制游戏（如重置）
echo "reset" > /sys/kernel/debug/tetris/control

