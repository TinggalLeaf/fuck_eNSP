//! eNSP error code knowledge base. Compiled from reverse-engineered client
//! strings plus widely-documented community experience. The protocol capture
//! in the 实时日志 tab is the authoritative source; these entries explain the
//! most common codes.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct KbEntry {
    pub code: i64,
    pub title: String,
    pub meaning: String,
    pub causes: Vec<String>,
    pub fixes: Vec<String>,
}

pub fn knowledge_base() -> Vec<KbEntry> {
    vec![
        KbEntry {
            code: 40,
            title: "启动虚拟设备失败（最常见）".into(),
            meaning: "eNSP 调用 VirtualBox 启动 AR/SW 等虚拟设备失败。\n★ 新版 Windows（2025-09 KB5124007/5124008 起，ntdll.dll ≥ 10.0.26100.9444）上的首要根因：VirtualBox 5.2.x 的进程加固自检（hardening）与新版 ntdll.dll 不兼容，错误码 -5607。失败发生在 supR3HardNtChildPurify 阶段（进程 Respawn 后的自我净化），此时 VMM 尚未初始化、根本没走到申请 VT-x 那一步——与 Hyper-V 无关。VBox 校验 ntdll 的 SizeOfImage（0x267000）与实际映射大小（0x26c000）的差超出 5.2 写死的容差，自检直接拒绝启动 VBoxHeadless。\n★ 经典根因：Hyper-V/VBS 与 VirtualBox 5.2.x 冲突（VBox 5.2 无 WHP 兼容层，需独占 VT-x）。".into(),
            causes: vec![
                "【加固不兼容 -5607】VBoxHardening.log 最后几行出现：Error (rc=-5607) / SizeOfImage isn't close enough to the mapping size / supR3HardNtChildPurify failed。伴随特征：SUPR3HardenedWinFindAdversaries: 0x0（排除杀软注入）；卸载 Hyper-V 重启后仍失败".into(),
                "【加固不兼容的日志签名】clonevm / snapshot / registervm 全部成功（纯磁盘配置操作，不拉起受加固保护的进程），只有 startvm 失败".into(),
                "【Hyper-V/VBS 冲突】HypervisorPresent=True；vmms/vmcompute 运行；或 VBS 引导阶段单独拉起 hypervisor（hvhost 常驻）独占 VT-x".into(),
                "AR_Base 等基础虚拟机未注册或注册信息损坏".into(),
                "缺少 Host-Only 虚拟网卡".into(),
                "杀毒软件拦截 VBoxHeadless".into(),
                "65500-65514 端口被占用".into(),
                "克隆机失败后被自动清理，VBoxHardening.log 消失，表现为'无日志可查'（需在启动失败瞬间到 %USERPROFILE%\\VirtualBox VMs\\<设备>\\Logs\\ 手动抢存）".into(),
            ],
            fixes: vec![
                "第一步：先判定是哪种根因。用本工具'一键体检'：ntdll 版本 ≥ 10.0.26100.9444 + VBox 5.2.x ⇒ 加固不兼容；或抓到 VBoxHardening.log 中有 -5607 ⇒ 加固不兼容".into(),
                "【加固不兼容唯一可行路径】VirtualBox 升级到 7.1.4+（-5607 在该版本修复）+ 开源垫片 LBXaaa/ensp-vbox-shim（伪装版本号 + vtable 映射 + 调用约定转换，让原版 eNSP 调用 VBox 7.x 引擎）".into(),
                "⚠️ 不要降级 5.2.44：同属 5.2 分支，未含 7.1.4 的校验修复，对本 build 无效".into(),
                "加固不兼容场景下 Hyper-V 可以恢复：bcdedit /set hypervisorlaunchtype auto + 重新勾选 Hyper-V/虚拟机平台/WSL。VBox 7 支持 WHP/NEM 后端与 Hyper-V 共存，垫片方案本就以'不关 Hyper-V'为目标，不必牺牲 WSL2/Docker/沙盒".into(),
                "【Hyper-V 冲突路径】bcdedit /set hypervisorlaunchtype off；内核隔离内存完整性=关；Windows 功能取消勾选 Hyper-V/虚拟机平台/WSL2；重启".into(),
                "eNSP 菜单：工具 -> 注册设备，重新注册 AR_Base / WLAN 基础镜像".into(),
                "查看'实时日志'中 WSASend/WSARecv 抓到的服务端返回详情，那是真正的失败原因".into(),
            ],
        },
        KbEntry {
            code: 41,
            title: "设备启动失败 / 端口占用".into(),
            meaning: "设备进程未能正常拉起，通常因为上次设备未正常关闭或相关端口/资源被占用。".into(),
            causes: vec![
                "上一次设备未正常关闭，残留 VBoxHeadless / VBoxSVC 进程".into(),
                "设备运行目录或临时文件被占用".into(),
                "eNSP 服务端端口(65510/65511)被残留进程占用".into(),
            ],
            fixes: vec![
                "任务管理器结束所有 VBoxHeadless.exe / VBoxSVC.exe / eNSP_VBoxServer.exe 后重启 eNSP".into(),
                "eNSP 菜单：工具 -> 选项 -> 服务，确认端口设置后重启 eNSP".into(),
                "重启电脑可清除所有残留占用".into(),
            ],
        },
        KbEntry {
            code: 42,
            title: "虚拟设备运行异常".into(),
            meaning: "设备虚拟机启动后未能通过内部自检，或设备代理进程异常退出。".into(),
            causes: vec![
                "虚拟设备镜像文件损坏".into(),
                "设备内存设置过大导致分配失败".into(),
                "VM 内代理(eNSP_Router.exe 等)启动超时".into(),
            ],
            fixes: vec![
                "删除该设备后重新拖入拓扑（会重新克隆基础镜像）".into(),
                "调小设备内存：选中设备 -> 右键设置".into(),
                "检查 C 盘剩余空间（设备运行目录在 %LOCALAPPDATA%\\eNSP）".into(),
            ],
        },
        KbEntry {
            code: 43,
            title: "内存不足".into(),
            meaning: "启动设备过多或系统可用内存不足，无法为新设备分配内存。".into(),
            causes: vec![
                "同时启动的设备过多".into(),
                "每台设备默认内存较大（路由器约 256MB+）".into(),
            ],
            fixes: vec![
                "减少同时启动的设备数量，分批启动".into(),
                "增加物理内存或调小单台设备内存".into(),
                "关闭其它占用内存的程序".into(),
            ],
        },
        KbEntry {
            code: 44,
            title: "VirtualBox 路径配置错误".into(),
            meaning: "eNSP 选项中配置的 VirtualBox 安装路径不正确。".into(),
            causes: vec!["VBox 安装后移动过目录，或 eNSP 中路径未更新".into()],
            fixes: vec![
                "eNSP 菜单：工具 -> 选项 -> 检查 VBox 路径，指向 VBoxManage.exe 所在目录".into(),
            ],
        },
        KbEntry {
            code: 1,
            title: "与服务端通信失败".into(),
            meaning: "客户端无法连接 eNSP_VBoxServer（监听端口 65510）。".into(),
            causes: vec![
                "vboxserver 未启动或被防火墙拦截".into(),
                "本机防火墙/安全软件拦截了 65500-65514 回环端口".into(),
            ],
            fixes: vec![
                "将 eNSP 加入防火墙白名单".into(),
                "工具 -> 选项 -> 服务 中核对端口，重启 eNSP".into(),
            ],
        },
        KbEntry {
            code: 27,
            title: "拷贝设备运行依赖文件失败".into(),
            meaning: "创建设备运行目录或拷贝依赖文件失败。".into(),
            causes: vec![
                "%LOCALAPPDATA%\\eNSP 目录权限不足或磁盘满".into(),
                "杀毒软件锁定文件".into(),
            ],
            fixes: vec![
                "以管理员身份运行 eNSP".into(),
                "清理 %LOCALAPPDATA%\\eNSP\\tmp 下的残留".into(),
                "检查 C 盘剩余空间".into(),
            ],
        },
        KbEntry {
            code: 28,
            title: "写入 hardcfg / 配置文件失败".into(),
            meaning: "创建设备硬件配置或压缩配置文件失败。".into(),
            causes: vec!["运行目录不可写或磁盘空间不足".into()],
            fixes: vec!["检查磁盘空间；以管理员身份运行；关闭杀毒软件重试".into()],
        },
        KbEntry {
            code: 0,
            title: "其它错误码".into(),
            meaning: "上面的条目没有覆盖的错误码。".into(),
            causes: vec!["见“实时日志”中服务端返回的原始报文和详细文本".into()],
            fixes: vec![
                "在“实时日志”里搜索 错误代码 / ErrorCode，查看同一时刻前后的 WSARecv 报文，里面有服务端返回的详细原因".into(),
                "把日志导出后联系华为支持或社区".into(),
            ],
        },
    ]
}
