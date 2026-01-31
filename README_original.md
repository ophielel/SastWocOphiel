# SAST WoC 2026 - Rust for Linux Kernel Playground

这是一个给SAST Linux/运维组新生准备的寒假上手项目喵。
它不是“读完文档照着做就结束”的那种题，而更像一块练习场：你会在一个最小可用的 BusyBox 系统里，加载内核模块、和字符设备交互、读内核日志、做一点点工程化。

你会接触到的关键词：Rust for Linux、内核模块、字符设备、ioctl、debugfs、QEMU、rootfs、CI/CD、OCI 镜像。

## 项目目录结构

```
woc2026-hello-from-skm/
├── src/
│   ├── module.rs              # Rust 内核模块入口
│   ├── tetris.rs              # /dev/tetris 字符设备 + 游戏逻辑
│   └── Kbuild                 # out-of-tree 构建配置
├── magic/
│   └── magic.c                # C 写的 magic 模块源码（本题 ioctl 协议来源）
├── tools/
│   └── play_tetris.c          # 用户态交互程序
├── scripts/
│   ├── setup.sh               # 初始化子模块并配置内核 Rust 环境
│   ├── run.sh                 # QEMU 启动脚本
│   └── config-rootfs.sh        # 配置 BusyBox rootfs（包含开机脚本）
├── Makefile                    # 统一构建入口
└── qemu-busybox-min.config     # 内核 config（包含 rust.config）
```

## 快速开始

### 你需要准备什么

建议在 Ubuntu/Debian 类环境完成。常见依赖：

- 构建工具链：`git make gcc clang ld.lld python3 bc bison flex pkg-config`
- rootfs 打包：`cpio gzip`
- QEMU：`qemu-system-x86_64`
- 生成 Rust 绑定：`bindgen`
- Rust 管理：`rustup`（`scripts/setup.sh` 会在缺失时尝试安装）

### 初始化 + 构建 + 启动

```bash
make setup
make build
make run
```

一些小提示：

- `scripts/run.sh` 默认启用 KVM（`-enable-kvm`）。如果你在没有 KVM 的环境（例如部分云主机/WSL），需要自己调整脚本参数。
- `make build` 会编译 kernel + busybox + 模块 + 打包 rootfs，第一次会比较慢。

## 进入 QEMU 之后，建议先做的几件事

把它当成一台“只有你知道发生了什么”的小机器，先学会观察：

- `dmesg`：内核日志是第一手线索（模块加载、设备创建、报错、flag 都可能在这里）
- `lsmod`：确认模块是否在
- `ls -l /dev`：确认你正在和哪个设备节点说话
- `cat /proc/devices`：当你不知道设备节点从哪来的，这个很有用

有一个已经写好的用户态程序在系统里，你可以试试运行它（由 `tools/play_tetris.c` 编译安装）：

```sh
play_tetris
```

## WoC Tasks

下面四件事是这份项目的主线。
你不需要一次做对；你只需要学会“观察 -> 推理 -> 改动 -> 验证”的循环。

### 任务 1：找到并修复 tetris 模块中的 panic

你要做的事情很简单：让 `woc2026_hello_from_skm.ko` 变得“可加载、可交互”。

建议的探索路径：

- 从 `src/module.rs` 的 `init()` 开始读：模块加载时到底做了什么？
- 确认 `/dev/tetris` 是在哪里注册的（提示：miscdevice）
- 学会用 `dmesg` 把“发生了什么”串成故事

不要怕 panic：它往往是最明确的线索。

### 任务 2：加载 magic.ko，用 ioctl 触发 flag

这题的重点不是“写很多代码”，而是把 ioctl 的用户态/内核态交互跑通，并知道如何定位协议。

#### magic ioctl 协议

源码位置：`magic/magic.c:1`

- 设备节点：`/dev/magic`
- ioctl 命令号：`0x1337`
- 参数：无（`arg` 未使用，传 `0` 即可）
- 返回值：
  - 成功返回 `0`
  - 命令号不匹配返回 `-EINVAL`
- flag 在哪里：成功时会通过 `printk` 输出到内核日志（也就是 `dmesg`）。

最小用户态调用示例（只展示关键逻辑）：

```c
int fd = open("/dev/magic", O_RDWR);
ioctl(fd, 0x1337, 0);
// 然后在 shell 里：dmesg | tail
```

建议你做得更“像真实世界”一点：

- 你的程序可以先检查 `/dev/magic` 是否存在，给出友好的提示
- 调用 ioctl 后自动提示用户去看 `dmesg`，或者直接帮用户执行 `dmesg`（如果你愿意）

顺便留意一个工程细节：rootfs 的开机脚本会尝试自动 `insmod` `magic.ko`，具体在 `scripts/config-rootfs.sh:22` 生成的 `rcS` 里。

### 任务 3：给这个仓库加上 CI/CD + 产出 OCI 镜像

把它当成一次“把随手能跑的东西，变成别人也能稳定复现”的练习。

你可以从这些问题出发：

- CI 里怎么拉子模块、怎么选择 Rust toolchain、怎么跑 `make build`？
- 能不能做一个最小的 smoke test：启动 QEMU，跑几条命令，能证明系统基本健康？
- 如何把构建产物（`bzImage`/`rootfs.img`/`*.ko`/用户态工具）存成 artifact？
- OCI 镜像要解决的核心是什么：让别人只需要 `docker run` 就能拉起 QEMU，并且能看到串口输出。

提示：CI 环境往往没有 KVM，记得让你的测试在纯 emulation 下也能跑。

### 任务 4：给 tetris 模块加一个 debugfs 调试入口

debugfs 很适合做“只用于调试/观测”的接口：不需要稳定 ABI，也不一定要对普通用户好用，但要对开发者诚实。

你可以按自己的习惯设计，例如：

- 读出当前状态（分数、game over、当前方块信息等）
- 读出棋盘（用你喜欢的文本格式）
- 做一个控制入口（写入 `left/right/reset` 之类的命令）

建议把目标定得小一点：先做一个只读文件，把它做稳定；再考虑写接口。

## 你可以继续改进的地方（加分项）

这些不要求必须做，但很值得做：

- 把 ioctl 命令号做成共享头文件/绑定，避免用户态和内核态各写各的魔数。
- 给 `scripts/run.sh` 加一个“无 KVM 的降级开关”。
- 做一个 `make test`：本地一键跑 QEMU smoke test。
- 给仓库加上基本的格式化/静态检查入口（`rustfmt`/`clippy`/`clang-format`）。

## 参考资料

- Rust for Linux：`https://docs.kernel.org/rust/`
- Linux kernel 文档：`https://www.kernel.org/doc/html/latest/`
- ioctl：`man 2 ioctl`
- debugfs（内核树）：`Documentation/filesystems/debugfs.rst`
