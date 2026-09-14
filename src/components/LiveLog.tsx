import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  Play,
  Syringe,
  Eraser,
  Download,
  Pause,
  Search,
  Radio,
} from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";
import { Switch } from "./ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import PageHeader from "./PageHeader";
import type { DiscoverInfo, HookEntry, InjectResult } from "../types";
import { fmtTime } from "../types";
import { cn } from "../lib/utils";

interface Props {
  onShowKb: (code: number) => void;
}

const MAX_ENTRIES = 10000;
const ROW_H = 26;

const PROC_PALETTE = [
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#a855f7",
  "#d946ef",
  "#f97316",
  "#eab308",
  "#84cc16",
];

function procColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 997;
  return PROC_PALETTE[h % PROC_PALETTE.length];
}

function ProcChip({ name, pid }: { name: string; pid: number }) {
  const c = procColor(name);
  return (
    <span
      className="shrink-0 rounded border px-1.5 font-mono text-[10.5px] leading-4"
      style={{
        color: c,
        borderColor: `${c}55`,
        background: `${c}14`,
      }}
    >
      {name}:{pid}
    </span>
  );
}

export default function LiveLog({ onShowKb }: Props) {
  const [entries, setEntries] = useState<HookEntry[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filter, setFilter] = useState("");
  const [level, setLevel] = useState<string>("all");
  const [procFilter, setProcFilter] = useState<string>("all");
  const [info, setInfo] = useState<DiscoverInfo | null>(null);
  const [working, setWorking] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(400);
  const viewRef = useRef<HTMLDivElement>(null);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  const autoRef = useRef(autoScroll);
  autoRef.current = autoScroll;

  useEffect(() => {
    let disposed = false;
    (async () => {
      const init = await invoke<HookEntry[]>("hook_logs_read");
      if (!disposed) setEntries(init.slice(-2000));
      await invoke("stream_start");
      if (!disposed) setStreaming(true);
    })();
    const un = listen<HookEntry[]>("hook-log", (ev) => {
      if (pausedRef.current) return;
      setEntries((prev) => {
        const next = prev.concat(ev.payload);
        return next.length > MAX_ENTRIES ? next.slice(next.length - MAX_ENTRIES) : next;
      });
    });
    const un2 = listen<InjectResult>("inject-event", (ev) => {
      if (ev.payload.ok) {
        toast.success(`已注入 ${ev.payload.name}`, {
          description: `pid ${ev.payload.pid}`,
        });
      }
    });
    return () => {
      disposed = true;
      un.then((f) => f());
      un2.then((f) => f());
      invoke("stream_stop");
    };
  }, []);

  useEffect(() => {
    const t = setInterval(async () => {
      setInfo(await invoke<DiscoverInfo>("discover"));
    }, 2000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    const el = viewRef.current;
    if (el) {
      setViewH(el.clientHeight);
      if (autoRef.current) {
        el.scrollTop = el.scrollHeight;
      }
    }
  }, [entries]);

  useEffect(() => {
    const el = viewRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const doLaunch = async () => {
    setWorking(true);
    try {
      const r = await invoke<InjectResult>("launch");
      if (r.ok) {
        toast.success(`已启动 ${r.name} 并完成注入`, { description: `pid ${r.pid}` });
      } else {
        toast.error(`启动失败：${r.message}`);
      }
    } catch (e) {
      toast.error(`启动失败：${e}`);
    } finally {
      setWorking(false);
    }
  };

  const doAttach = async () => {
    setWorking(true);
    try {
      const rs = await invoke<InjectResult[]>("attach");
      const fails = rs.filter((r) => !r.ok && r.pid !== 0);
      const oks = rs.filter((r) => r.ok && r.pid !== 0);
      if (oks.length) toast.success(`已注入：${oks.map((r) => r.name).join(", ")}`);
      if (fails.length) {
        toast.warning(fails.map((r) => `${r.name}: ${r.message}`).join("；"));
      }
      if (!oks.length && !fails.length) {
        toast.info("未发现运行中的 eNSP 进程，请先用「启动并 Hook eNSP」");
      }
    } catch (e) {
      toast.error(`注入失败：${e}`);
    } finally {
      setWorking(false);
    }
  };

  const doClear = async () => {
    await invoke("hook_logs_clear");
    setEntries([]);
    toast.info("日志已清空");
  };

  const doExport = () => {
    const text = entries
      .map((e) => `[${fmtTime(e.ts)}] [${e.proc}:${e.pid}] [${e.api}] ${e.text}`)
      .join("\n");
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `fuck_ensp_logs_${Date.now()}.txt`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  const procs = useMemo(
    () => Array.from(new Set(entries.map((e) => e.proc))).sort(),
    [entries],
  );

  const filtered = useMemo(
    () =>
      entries.filter((e) => {
        if (level !== "all" && e.level !== level) return false;
        if (procFilter !== "all" && e.proc !== procFilter) return false;
        if (filter) {
          const f = filter.toLowerCase();
          return `${e.proc} ${e.api} ${e.peer} ${e.text}`.toLowerCase().includes(f);
        }
        return true;
      }),
    [entries, level, procFilter, filter],
  );

  const errCount = useMemo(() => entries.filter((e) => e.level === "error").length, [entries]);
  const warnCount = useMemo(() => entries.filter((e) => e.level === "warn").length, [entries]);

  // windowing
  const total = filtered.length;
  const start = Math.max(0, Math.floor(scrollTop / ROW_H) - 20);
  const end = Math.min(total, Math.ceil((scrollTop + viewH) / ROW_H) + 20);
  const slice = filtered.slice(start, end);

  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (!atBottom && autoRef.current) setAutoScroll(false);
    if (atBottom && !autoRef.current) setAutoScroll(true);
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <PageHeader
        title={
          <>
            实时日志
            {streaming && (
              <Badge variant="success" className="ml-2.5">
                <Radio className="size-3" /> LIVE
              </Badge>
            )}
          </>
        }
        desc="Hook eNSP 客户端 / 服务端协议流，错误码级根因透视"
      >
        <Button onClick={doLaunch} disabled={working}>
          <Play /> 启动并 Hook eNSP
        </Button>
        <Button variant="secondary" onClick={doAttach} disabled={working}>
          <Syringe /> Hook 运行中的
        </Button>
      </PageHeader>

      {/* toolbar */}
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" onClick={doClear}>
          <Eraser /> 清空
        </Button>
        <Button variant="outline" size="sm" onClick={doExport}>
          <Download /> 导出
        </Button>
        <Button
          variant={paused ? "destructive" : "outline"}
          size="sm"
          onClick={() => setPaused(!paused)}
        >
          {paused ? <Play /> : <Pause />}
          {paused ? "继续" : "暂停"}
        </Button>
        <span className="mx-1 h-5 w-px bg-border" />
        <Select value={level} onValueChange={setLevel}>
          <SelectTrigger className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部级别</SelectItem>
            <SelectItem value="info">info</SelectItem>
            <SelectItem value="warn">warn</SelectItem>
            <SelectItem value="error">error</SelectItem>
          </SelectContent>
        </Select>
        <Select value={procFilter} onValueChange={setProcFilter}>
          <SelectTrigger className="w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部进程</SelectItem>
            {procs.map((p) => (
              <SelectItem key={p} value={p}>
                {p}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="搜索日志内容…"
            className="w-60 pl-8"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
      </div>

      {/* status strip */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 rounded-lg border border-border bg-white/[0.025] px-3 py-1.5 text-[11.5px] text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <span
            className={cn(
              "size-1.5 rounded-full",
              streaming ? "bg-emerald-400 animate-pulse-dot" : "bg-white/25",
            )}
          />
          {streaming ? "监控中" : "未启动"}
        </span>
        <span>
          已注入：
          {info && info.hooked_pids.length
            ? info.hooked_pids.map((p) => `#${p}`).join(", ")
            : "无"}
        </span>
        <span className="truncate">
          eNSP 进程：
          {info && info.ensp_processes.length
            ? info.ensp_processes.map((p) => `${p.name}(${p.pid})`).join(", ")
            : "无"}
        </span>
        <span>共 {entries.length} 条</span>
        {errCount > 0 && <Badge variant="destructive">{errCount} 错误</Badge>}
        {warnCount > 0 && <Badge variant="warning">{warnCount} 警告</Badge>}
        <span className="ml-auto flex items-center gap-2">
          自动滚动
          <Switch checked={autoScroll} onCheckedChange={setAutoScroll} />
        </span>
      </div>

      {/* log stream */}
      <div
        ref={viewRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-y-auto rounded-xl border border-border bg-black/45 font-mono text-[12px] shadow-[0_1px_0_0_rgb(255_255_255/0.03)_inset]"
      >
        {total === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
            <Radio className="size-7 opacity-30" />
            <span className="text-xs">暂无日志。点击上方「启动并 Hook eNSP」开始抓取</span>
          </div>
        ) : (
          <div className="relative" style={{ height: total * ROW_H }}>
            {slice.map((e, i) => {
              const idx = start + i;
              const isErr = e.level === "error";
              const isWarn = e.level === "warn";
              return (
                <div
                  key={idx}
                  className={cn(
                    "absolute left-0 right-0 flex items-center gap-2.5 px-2.5 whitespace-nowrap",
                    isErr && "border-l-2 border-red-400 bg-red-500/[0.08]",
                    isWarn && "border-l-2 border-amber-400/80 bg-amber-400/[0.05]",
                    !isErr && !isWarn && "border-l-2 border-transparent",
                  )}
                  style={{ top: idx * ROW_H, height: ROW_H }}
                >
                  <span className="shrink-0 text-muted-foreground/80">{fmtTime(e.ts)}</span>
                  <ProcChip name={e.proc} pid={e.pid} />
                  <span
                    className={cn(
                      "shrink-0 rounded border px-1.5 text-[10.5px] leading-4",
                      e.cat === "proc"
                        ? "border-primary/40 bg-primary/10 text-orange-300"
                        : e.cat === "sys"
                          ? "border-border bg-white/5 text-muted-foreground"
                          : "border-sky-400/30 bg-sky-400/10 text-sky-300",
                    )}
                  >
                    {e.api}
                  </span>
                  {e.dir && (
                    <span
                      className={cn(
                        "w-3 shrink-0",
                        e.dir === "in" ? "text-emerald-400" : "text-sky-400",
                      )}
                    >
                      {e.dir === "in" ? "→" : "←"}
                    </span>
                  )}
                  {e.peer && <span className="shrink-0 text-muted-foreground">{e.peer}</span>}
                  {e.cat === "proc" && (
                    <span
                      className={cn(
                        "shrink-0 rounded border px-1.5 text-[10.5px] leading-4",
                        e.ok
                          ? "border-emerald-400/30 bg-emerald-400/10 text-emerald-300"
                          : "border-red-400/40 bg-red-400/10 text-red-300",
                      )}
                    >
                      {e.ok ? `spawn #${e.child_pid ?? "?"}` : "FAIL"}
                    </span>
                  )}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span
                        className={cn(
                          "min-w-0 flex-1 overflow-hidden text-ellipsis",
                          isErr ? "text-red-300" : isWarn ? "text-amber-200" : "text-[#d6deeb]",
                        )}
                      >
                        {e.text}
                      </span>
                    </TooltipTrigger>
                    <TooltipContent side="top" align="start" className="max-w-150 font-mono">
                      {e.text}
                    </TooltipContent>
                  </Tooltip>
                  {e.err_code != null && (
                    <button
                      onClick={() => onShowKb(e.err_code!)}
                      className="shrink-0 cursor-pointer rounded border border-red-400/40 bg-red-400/10 px-1.5 text-[10.5px] leading-4 text-red-300 transition-colors hover:bg-red-400/25"
                    >
                      错误码 {e.err_code} →
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
