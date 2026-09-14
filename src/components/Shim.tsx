import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { motion } from "framer-motion";
import {
  Puzzle,
  Zap,
  ListChecks,
  Undo2,
  RefreshCw,
  ShieldCheck,
  OctagonX,
  Download,
  ExternalLink,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Info,
  Terminal,
} from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Progress } from "./ui/progress";
import { Card, CardContent } from "./ui/card";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./ui/alert-dialog";
import PageHeader from "./PageHeader";
import { cn } from "../lib/utils";

interface ShimItem {
  id: string;
  title: string;
  status: "pass" | "warn" | "fail" | "info";
  detail: string;
  fix?: string;
}

interface ShimStatus {
  items: ShimItem[];
  installed: boolean;
  vbox_version: string;
  ensp_dir: string;
  vbox_dir: string;
  shim_version: string;
}

interface SoftInfo {
  found: boolean;
  path: string;
  version: string;
  source: string;
}

interface ConflictProc {
  name: string;
  pid: number;
}

interface ShimPreflight {
  ensp: SoftInfo;
  vbox: SoftInfo;
  conflicts: ConflictProc[];
  blockers: string[];
  ready: boolean;
}

const META: Record<string, { text: string; icon: React.ElementType; cls: string; badge: "success" | "warning" | "destructive" | "outline" }> = {
  pass: { text: "就绪", icon: CheckCircle2, cls: "text-emerald-400", badge: "success" },
  warn: { text: "警告", icon: AlertTriangle, cls: "text-amber-400", badge: "warning" },
  fail: { text: "缺失", icon: XCircle, cls: "text-red-400", badge: "destructive" },
  info: { text: "信息", icon: Info, cls: "text-muted-foreground", badge: "outline" },
};

type ConfirmKind = "install" | "uninstall" | "kill-install";

export default function Shim() {
  const [status, setStatus] = useState<ShimStatus | null>(null);
  const [pf, setPf] = useState<ShimPreflight | null>(null);
  const [killing, setKilling] = useState(false);
  const [running, setRunning] = useState<string | null>(null); // install | uninstall | register
  const [log, setLog] = useState<string[]>([]);
  const [confirm, setConfirm] = useState<{ kind: ConfirmKind; conflicts?: ConflictProc[] } | null>(null);
  const logRef = useRef<HTMLDivElement>(null);

  const refresh = async () => {
    const [s, p] = await Promise.all([
      invoke<ShimStatus>("shim_status"),
      invoke<ShimPreflight>("shim_preflight"),
    ]);
    setStatus(s);
    setPf(p);
  };

  useEffect(() => {
    refresh();
    const un = listen<string>("shim-log", (ev) => {
      setLog((p) => [...p.slice(-499), ev.payload]);
    });
    const un2 = listen<{ code: number; error?: string }>("shim-exit", (ev) => {
      setRunning(null);
      if (ev.payload.code === 0) {
        toast.success("任务完成");
      } else {
        toast.error(`任务退出码 ${ev.payload.code}${ev.payload.error ? "：" + ev.payload.error : ""}`);
      }
      refresh();
    });
    return () => {
      un.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  const startInstall = async () => {
    setLog([]);
    setRunning("install");
    try {
      await invoke("shim_install");
    } catch (e) {
      toast.error(String(e));
      setRunning(null);
    }
  };

  // 前置判断：装得对不对位置、依赖齐不齐、有没有进程在锁文件
  const doInstall = async () => {
    const cur = await invoke<ShimPreflight>("shim_preflight");
    setPf(cur);

    const hardBlocked =
      !cur.ensp.found || !cur.vbox.found || !cur.vbox.version.startsWith("7.2");
    if (hardBlocked) {
      toast.error("前置条件不满足，无法安装", {
        description:
          cur.blockers.filter((b) => !b.includes("冲突进程")).join("\n") +
          "\n\n装齐后再试（VBox 7.2.x 官方直链见下方按钮）",
        duration: 8000,
      });
      return;
    }

    if (cur.conflicts.length > 0) {
      setConfirm({ kind: "kill-install", conflicts: cur.conflicts });
      return;
    }
    setConfirm({ kind: "install" });
  };

  const doRegister = async () => {
    setLog([]);
    setRunning("register");
    try {
      await invoke("shim_register_vms");
    } catch (e) {
      toast.error(String(e));
      setRunning(null);
    }
  };

  const killAll = async () => {
    setKilling(true);
    try {
      const left = await invoke<ConflictProc[]>("shim_kill_conflicts");
      if (left.length === 0) toast.success("冲突进程已全部结束");
      else toast.warning(`仍有 ${left.length} 个未能结束：${left.map((l) => l.name).join(", ")}`);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setKilling(false);
    }
  };

  const onConfirm = async () => {
    const c = confirm;
    setConfirm(null);
    if (!c) return;
    if (c.kind === "uninstall") {
      setLog([]);
      setRunning("uninstall");
      try {
        await invoke("shim_uninstall");
      } catch (e) {
        toast.error(String(e));
        setRunning(null);
      }
    } else if (c.kind === "install") {
      startInstall();
    } else {
      setKilling(true);
      try {
        const left = await invoke<ConflictProc[]>("shim_kill_conflicts");
        await refresh();
        if (left.length > 0) {
          toast.warning(`以下进程未能结束：${left.map((l) => l.name).join(", ")}，请手动关闭后再试`);
          return;
        }
      } catch (e) {
        toast.error(String(e));
        return;
      } finally {
        setKilling(false);
      }
      startInstall();
    }
  };

  const ready = status?.installed;
  const vboxOk = pf ? pf.vbox.found && pf.vbox.version.startsWith("7.2") : false;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <PageHeader
        title={
          <>
            shim 一键
            <Badge variant="accent" className="ml-2.5">
              <Puzzle /> {status?.shim_version ?? ""} 已内置
            </Badge>
          </>
        }
        desc="让原版 eNSP 直接跑在 VirtualBox 7.x 上：VBox52.dll 垫片 + vtable 映射，不降级、不关 Hyper-V"
      >
        <div className="flex flex-col items-end">
          <span className={cn("text-base font-bold leading-tight", ready ? "text-emerald-400" : "text-amber-300")}>
            {ready ? "已安装" : "未安装"}
          </span>
          <span className="text-[10px] uppercase tracking-widest text-muted-foreground">install state</span>
        </div>
      </PageHeader>

      {/* 安装前检查 */}
      {pf && (
        <Card
          className={cn(
            "shrink-0",
            pf.ready
              ? "border-emerald-400/30"
              : pf.ensp.found && vboxOk
                ? "border-amber-400/30"
                : "border-red-400/30",
          )}
        >
          <CardContent className="space-y-2.5 p-3.5">
            <div className="flex items-center gap-2 text-[13px] font-semibold">
              <ShieldCheck
                className={cn(
                  "size-4",
                  pf.ready ? "text-emerald-400" : pf.conflicts.length ? "text-amber-400" : "text-red-400",
                )}
              />
              安装前检查
              {pf.ready ? (
                <Badge variant="success">全部就绪</Badge>
              ) : (
                <Badge variant={pf.ensp.found && vboxOk ? "warning" : "destructive"}>
                  {pf.blockers.length} 项待处理
                </Badge>
              )}
              <Button
                variant="ghost"
                size="sm"
                className="ml-auto"
                onClick={refresh}
                disabled={!!running}
              >
                <RefreshCw /> 重新检查
              </Button>
            </div>

            <div className="grid grid-cols-1 gap-2 xl:grid-cols-2">
              <div className="flex min-w-0 items-center gap-2 text-xs">
                <span className="shrink-0 text-muted-foreground">eNSP 安装位置</span>
                {pf.ensp.found ? (
                  <>
                    <code className="truncate rounded border border-border bg-black/40 px-1.5 py-0.5 font-mono text-[11px] text-sky-300" title={pf.ensp.path}>
                      {pf.ensp.path}
                    </code>
                    {pf.ensp.version && <Badge variant="outline">{pf.ensp.version}</Badge>}
                    <Badge variant="info">{pf.ensp.source}</Badge>
                  </>
                ) : (
                  <span className="text-red-300">未检测到，请先安装华为 eNSP</span>
                )}
              </div>
              <div className="flex min-w-0 items-center gap-2 text-xs">
                <span className="shrink-0 text-muted-foreground">VirtualBox 安装位置</span>
                {pf.vbox.found ? (
                  <>
                    <code className="truncate rounded border border-border bg-black/40 px-1.5 py-0.5 font-mono text-[11px] text-sky-300" title={pf.vbox.path}>
                      {pf.vbox.path}
                    </code>
                    {pf.vbox.version && (
                      <Badge variant={vboxOk ? "success" : "destructive"}>{pf.vbox.version}</Badge>
                    )}
                    <Badge variant="info">{pf.vbox.source}</Badge>
                  </>
                ) : (
                  <span className="text-red-300">未检测到</span>
                )}
              </div>
            </div>

            {pf.conflicts.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5">
                <span className="text-xs text-amber-300">
                  冲突进程 {pf.conflicts.length} 个（会锁定待替换的文件）：
                </span>
                {pf.conflicts.map((c) => (
                  <code key={`${c.name}-${c.pid}`} className="rounded border border-amber-400/30 bg-amber-400/10 px-1.5 py-px font-mono text-[10.5px] text-amber-300">
                    {c.name} · {c.pid}
                  </code>
                ))}
                <Button variant="destructive" size="sm" loading={killing} disabled={!!running} onClick={killAll}>
                  <OctagonX /> 全部结束
                </Button>
              </div>
            )}

            {!vboxOk && (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-xs text-red-300">
                  shim 需要官方 VirtualBox 7.2.x（当前{pf.vbox.found ? ` ${pf.vbox.version}` : "未安装"}）
                </span>
                <Button variant="outline" size="sm" onClick={() => openUrl("https://www.virtualbox.org/wiki/Downloads")}>
                  <ExternalLink /> 官方下载页
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openUrl("https://download.virtualbox.org/virtualbox/7.2.8/VirtualBox-7.2.8-173730-Win.exe")}
                >
                  <Download /> 直链 7.2.8（与垫片引擎一致）
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* actions */}
      <div className="flex shrink-0 flex-wrap items-center gap-2">
        <Button onClick={doInstall} disabled={!!running}>
          <Zap /> 一键安装 shim（打补丁+注册设备）
        </Button>
        <Button variant="secondary" onClick={doRegister} loading={running === "register"} disabled={!!running}>
          <ListChecks /> 仅核对/注册基础设备
        </Button>
        <Button
          variant="destructive"
          onClick={() => setConfirm({ kind: "uninstall" })}
          loading={running === "uninstall"}
          disabled={!!running}
        >
          <Undo2 /> 卸载还原
        </Button>
        {running && <Progress active value={100} className="w-36" />}
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 lg:grid-cols-12">
        {/* checklist */}
        <Card className="flex min-h-0 flex-col overflow-hidden lg:col-span-5">
          <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5 text-[13px] font-semibold">
            <ListChecks className="size-4 text-muted-foreground" />
            检测项
            <Badge variant="outline">{status?.items.length ?? 0}</Badge>
          </div>
          <div className="min-h-0 flex-1 space-y-1.5 overflow-y-auto p-2.5">
            {!status && <div className="p-3 text-xs text-muted-foreground">检测中…</div>}
            {status?.items.map((it, i) => {
              const m = META[it.status];
              const Icon = m.icon;
              return (
                <motion.div
                  key={it.id}
                  initial={{ opacity: 0, x: -10 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ duration: 0.2, delay: Math.min(i * 0.025, 0.4) }}
                  className="rounded-lg border border-border/70 bg-white/[0.02] px-2.5 py-2"
                >
                  <div className="flex items-center gap-2">
                    <Icon className={cn("size-3.5 shrink-0", m.cls)} />
                    <span className="text-xs font-medium">{it.title}</span>
                    <Badge variant={m.badge} className="ml-auto">{m.text}</Badge>
                  </div>
                  <div className="mt-1 pl-5.5 text-[11px] leading-relaxed whitespace-pre-wrap text-muted-foreground">
                    {it.detail}
                    {it.fix && it.status !== "pass" && it.status !== "info" && (
                      <span className="mt-1 block text-amber-200/80">→ {it.fix}</span>
                    )}
                  </div>
                </motion.div>
              );
            })}
          </div>
        </Card>

        {/* terminal output */}
        <Card className="flex min-h-0 flex-col overflow-hidden lg:col-span-7">
          <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5">
            <Terminal className="size-4 text-muted-foreground" />
            <span className="text-[13px] font-semibold">执行输出</span>
            {running && <Badge variant="info">运行中</Badge>}
            <span className="ml-auto flex gap-1.5">
              <span className="size-2.5 rounded-full bg-white/10" />
              <span className="size-2.5 rounded-full bg-white/10" />
              <span className="size-2.5 rounded-full bg-emerald-400/40" />
            </span>
          </div>
          <div
            ref={logRef}
            className="min-h-0 flex-1 overflow-y-auto bg-black/50 p-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-[#c9d4e3]"
          >
            {log.length === 0 ? (
              <span className="text-muted-foreground/70">
                点击上方按钮开始。安装过程约 10-30 秒，输出实时显示在这里。
              </span>
            ) : (
              log.join("\n")
            )}
          </div>
        </Card>
      </div>

      {/* confirm dialogs */}
      <AlertDialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {confirm?.kind === "uninstall"
                ? "卸载 shim 并还原？"
                : confirm?.kind === "kill-install"
                  ? `检测到 ${confirm.conflicts?.length ?? 0} 个冲突进程`
                  : "一键安装 ensp-vbox-shim？"}
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div>
                {confirm?.kind === "uninstall" && (
                  <span>
                    恢复 VBox52.dll 各位置、注册表版本、CLSID、VAR_Plugin.dll（从 .bak）到出厂状态。
                  </span>
                )}
                {confirm?.kind === "install" && (
                  <span>
                    将自动执行：部署 VBox52.dll 垫片（4 个位置）→ 注册表版本伪装 → CLSID 劫持 →
                    AR 插件补丁 → x86 运行时 → vboxserver 授权 → 按需注册基础设备 VM。全程可逆。
                  </span>
                )}
                {confirm?.kind === "kill-install" && (
                  <div className="space-y-2">
                    <pre className="max-h-40 overflow-y-auto rounded-md border border-border bg-black/50 p-2.5 font-mono text-[11.5px] text-amber-200">
                      {confirm.conflicts?.map((c) => `${c.name} (PID ${c.pid})`).join("\n")}
                    </pre>
                    <span className="text-amber-300/90">
                      这些进程会锁定 shim 需要替换的文件。将自动结束它们后继续安装；eNSP 里未保存的拓扑请先保存。
                    </span>
                  </div>
                )}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              variant={confirm?.kind === "uninstall" || confirm?.kind === "kill-install" ? "destructive" : "default"}
              onClick={onConfirm}
            >
              {confirm?.kind === "uninstall"
                ? "卸载还原"
                : confirm?.kind === "kill-install"
                  ? "结束进程并安装"
                  : "开始安装"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
