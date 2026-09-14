import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence, useSpring, useTransform } from "framer-motion";
import {
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Info,
  RefreshCw,
  ChevronRight,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Progress } from "./ui/progress";
import { Card } from "./ui/card";
import PageHeader from "./PageHeader";
import type { CheckItem } from "../types";
import { cn } from "../lib/utils";

const STATUS_META: Record<
  string,
  { text: string; color: string; icon: React.ElementType; badge: "success" | "warning" | "destructive" | "outline" }
> = {
  pass: { text: "通过", color: "text-emerald-400", icon: CheckCircle2, badge: "success" },
  warn: { text: "警告", color: "text-amber-400", icon: AlertTriangle, badge: "warning" },
  fail: { text: "失败", color: "text-red-400", icon: XCircle, badge: "destructive" },
  info: { text: "信息", color: "text-muted-foreground", icon: Info, badge: "outline" },
};

interface ItemEvent {
  idx: number;
  total: number;
  step: string;
  item: CheckItem;
}

function AnimatedScore({ value, running, pct }: { value: number; running: boolean; pct: number }) {
  const spring = useSpring(running ? 0 : value, { stiffness: 90, damping: 20 });
  useEffect(() => {
    spring.set(running ? 0 : value);
  }, [value, running, spring]);
  const display = useTransform(spring, (v) => `${Math.round(v)}`);
  const shown = running ? pct : value;
  const C = 2 * Math.PI * 40;
  const color = shown >= 80 ? "#34d399" : shown >= 50 ? "#fbbf24" : "#fb4d63";
  return (
    <div className="relative size-24 shrink-0">
      <svg viewBox="0 0 96 96" className="size-24 -rotate-90">
        <circle cx="48" cy="48" r="40" fill="none" stroke="rgb(255 255 255 / 0.07)" strokeWidth="8" />
        <motion.circle
          cx="48"
          cy="48"
          r="40"
          fill="none"
          stroke={running ? "url(#scan-grad)" : color}
          strokeWidth="8"
          strokeLinecap="round"
          strokeDasharray={C}
          animate={{ strokeDashoffset: C * (1 - Math.min(shown, 100) / 100) }}
          transition={{ type: "spring", stiffness: 60, damping: 18 }}
        />
        <defs>
          <linearGradient id="scan-grad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor="#38bdf8" />
            <stop offset="100%" stopColor="#34d399" />
          </linearGradient>
        </defs>
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        {running ? (
          <motion.span
            key={pct}
            initial={{ opacity: 0.4, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-lg font-bold text-sky-300"
          >
            {pct}%
          </motion.span>
        ) : (
          <motion.span className="text-2xl font-bold" style={{ color }}>
            {display}
          </motion.span>
        )}
        <span className="text-[9.5px] uppercase tracking-widest text-muted-foreground">
          {running ? "scanning" : "score"}
        </span>
      </div>
    </div>
  );
}

function CheckRow({ item, defaultOpen }: { item: CheckItem; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const meta = STATUS_META[item.status];
  const Icon = meta.icon;
  const expandable = !!item.fix || item.status === "fail" || item.status === "warn" || item.id === "conclusion";

  useEffect(() => setOpen(defaultOpen), [defaultOpen]);

  return (
    <motion.div
      layout="position"
      initial={{ opacity: 0, x: -14 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.25, ease: "easeOut" }}
      className={cn(
        "overflow-hidden rounded-lg border bg-card/70 transition-colors",
        item.id === "conclusion"
          ? "border-primary/35 bg-linear-to-r from-primary/[0.09] to-transparent"
          : open
            ? "border-border bg-card"
            : "border-border/70 hover:border-border",
      )}
    >
      <button
        onClick={() => expandable && setOpen(!open)}
        className={cn(
          "flex w-full items-center gap-2.5 px-3 py-2 text-left",
          expandable && "cursor-pointer",
        )}
      >
        <Icon className={cn("size-4 shrink-0", meta.color)} />
        <span className="text-[13px] font-medium">{item.title}</span>
        <Badge variant={meta.badge}>{meta.text}</Badge>
        {expandable && (
          <ChevronRight
            className={cn(
              "ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-200",
              open && "rotate-90",
            )}
          />
        )}
      </button>
      <AnimatePresence initial={false}>
        {open && (item.detail || item.fix) && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          >
            <div className="space-y-2 px-3 pb-3 pl-9.5">
              {item.detail && (
                <pre className="max-h-52 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-black/40 p-2.5 font-mono text-[11.5px] leading-relaxed text-muted-foreground">
                  {item.detail}
                </pre>
              )}
              {item.fix && (
                <div className="flex gap-2 rounded-md border border-amber-400/25 bg-amber-400/[0.06] p-2.5">
                  <Wrench className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
                  <pre className="whitespace-pre-wrap font-mono text-[11.5px] leading-relaxed text-amber-200/90">
                    {item.fix}
                  </pre>
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

export default function Checkup() {
  const [items, setItems] = useState<CheckItem[]>([]);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 15, step: "" });
  const [done, setDone] = useState(false);
  const runningRef = useRef(false);
  runningRef.current = running;

  const run = async () => {
    if (runningRef.current) return;
    setRunning(true);
    setDone(false);
    setItems([]);
    setProgress({ done: 0, total: 15, step: "准备中…" });
    await invoke("run_checkup");
  };

  useEffect(() => {
    const un = listen<ItemEvent>("checkup-item", (ev) => {
      const { idx, total, step, item } = ev.payload;
      setProgress({ done: idx + 1, total, step });
      setItems((prev) => {
        if (prev.some((p) => p.id === item.id)) return prev;
        return [...prev, item];
      });
    });
    const un2 = listen("checkup-done", () => {
      setRunning(false);
      setDone(true);
    });
    run();
    return () => {
      un.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  const counts = { pass: 0, warn: 0, fail: 0, info: 0 };
  items.forEach((i) => counts[i.status as keyof typeof counts]++);
  const score = Math.max(0, 100 - counts.fail * 30 - counts.warn * 10);
  const pct = progress.total ? Math.round((progress.done / progress.total) * 100) : 0;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <PageHeader title="一键体检" desc="eNSP 运行环境全量诊断：Hyper-V / VBS / VirtualBox / 加固日志 / 网卡 / 权限" />

      {/* hero */}
      <Card className="relative shrink-0 overflow-hidden">
        <div className="pointer-events-none absolute inset-0 bg-linear-to-r from-sky-500/[0.07] via-transparent to-emerald-500/[0.06]" />
        <div className="relative flex items-center gap-6 p-4 flex-wrap">
          <AnimatedScore value={score} running={running} pct={pct} />
          <div className="min-w-55 flex-1">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <ShieldCheck className={cn("size-4", running ? "text-sky-400" : counts.fail ? "text-red-400" : "text-emerald-400")} />
              {running ? "正在体检…" : done ? "体检完成" : "eNSP 环境体检"}
            </div>
            <Progress
              className="mt-2.5"
              value={running ? pct : 100}
              active={running}
              indicatorClassName={
                running
                  ? undefined
                  : counts.fail > 0
                    ? "bg-linear-to-r from-red-500 to-rose-400"
                    : counts.warn > 0
                      ? "bg-linear-to-r from-amber-500 to-yellow-400"
                      : undefined
              }
            />
            <div className="mt-1.5 text-xs text-muted-foreground">
              {running
                ? `当前步骤：${progress.step}`
                : `通过 ${counts.pass} · 警告 ${counts.warn} · 失败 ${counts.fail} · 共 ${items.length} 项`}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {!running && done && counts.fail > 0 && (
              <Badge variant="destructive">{counts.fail} 项失败</Badge>
            )}
            <Button variant="secondary" onClick={run} disabled={running}>
              <RefreshCw className={running ? "animate-spin" : ""} /> 重新体检
            </Button>
          </div>
        </div>
      </Card>

      {/* results */}
      <div className="flex-1 space-y-2 overflow-y-auto pr-0.5 min-h-0">
        {items.length === 0 && running && (
          <div className="flex items-center gap-2 pt-4 text-xs text-muted-foreground">
            <RefreshCw className="size-3.5 animate-spin" /> 正在收集环境信息…
          </div>
        )}
        {items.map((it) => (
          <CheckRow
            key={it.id}
            item={it}
            defaultOpen={it.status === "fail" || it.id === "conclusion"}
          />
        ))}
        {done && counts.fail > 0 && (
          <div className="mt-2 flex items-start gap-2 rounded-lg border border-red-400/30 bg-red-400/[0.07] p-3 text-xs text-red-200">
            <XCircle className="mt-0.5 size-3.5 shrink-0" />
            优先处理「综合结论」给出的修复步骤，处理完后重启电脑再重新体检。
          </div>
        )}
      </div>
    </div>
  );
}
