import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Activity,
  Stethoscope,
  BookOpen,
  Puzzle,
  FolderOpen,
  Bug,
} from "lucide-react";
import LiveLog from "./components/LiveLog";
import Checkup from "./components/Checkup";
import Kb from "./components/Kb";
import EnspLogs from "./components/EnspLogs";
import Shim from "./components/Shim";
import { Toaster } from "./components/ui/sonner";
import { TooltipProvider } from "./components/ui/tooltip";
import { cn } from "./lib/utils";

type TabKey = "live" | "checkup" | "kb" | "shim" | "ensplogs";

const NAV: { key: TabKey; label: string; desc: string; icon: React.ElementType }[] = [
  { key: "live", label: "实时日志", desc: "Hook 协议流", icon: Activity },
  { key: "checkup", label: "一键体检", desc: "环境诊断", icon: Stethoscope },
  { key: "kb", label: "错误码手册", desc: "40/41 根因", icon: BookOpen },
  { key: "shim", label: "shim 一键", desc: "VBox 7 垫片", icon: Puzzle },
  { key: "ensplogs", label: "eNSP 日志", desc: "日志文件", icon: FolderOpen },
];

/** brand mark: magnifier over an orange bug (matches the app icon) */
function Logo({ size = 30 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" fill="none">
      <rect width="48" height="48" rx="11" fill="#0b0d13" />
      <rect x="0.5" y="0.5" width="47" height="47" rx="10.5" stroke="white" strokeOpacity="0.1" />
      <circle cx="21.5" cy="20.5" r="11" stroke="#e8ecf4" strokeWidth="3.4" />
      <path d="M30 29 L39 38" stroke="#e8ecf4" strokeWidth="4.2" strokeLinecap="round" />
      <ellipse cx="21.5" cy="22" rx="4.6" ry="5.4" fill="#f97316" />
      <circle cx="21.5" cy="14.8" r="2.6" fill="#f97316" />
      <path d="M19.6 12.6 L17.8 10.4 M23.4 12.6 L25.2 10.4" stroke="#f97316" strokeWidth="1.4" strokeLinecap="round" />
      <path d="M17.4 21 L13.8 19.4 M25.6 21 L29.2 19.4 M17.2 24 L13.6 25.2 M25.8 24 L29.4 25.2" stroke="#f97316" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  );
}

export default function App() {
  const [tab, setTab] = useState<TabKey>("live");
  const [kbHighlight, setKbHighlight] = useState<number | null>(null);

  const showKb = (code: number) => {
    setKbHighlight(code);
    setTab("kb");
  };

  return (
    <TooltipProvider delayDuration={350}>
      <div className="app-bg flex h-full overflow-hidden">
        {/* ---------------- sidebar ---------------- */}
        <aside className="flex w-55 shrink-0 flex-col border-r border-border bg-black/25 backdrop-blur-sm">
          <div className="flex items-center gap-2.5 px-4 pt-4 pb-3">
            <Logo />
            <div className="min-w-0">
              <div className="text-[15px] font-bold leading-tight tracking-tight">
                fuck_ensp
              </div>
              <div className="text-[10.5px] text-muted-foreground tracking-wide">
                eNSP 万能错误排查
              </div>
            </div>
          </div>

          <nav className="flex flex-col gap-1 px-2.5 mt-1">
            {NAV.map((n) => {
              const active = tab === n.key;
              const Icon = n.icon;
              return (
                <button
                  key={n.key}
                  onClick={() => setTab(n.key)}
                  className={cn(
                    "group relative flex items-center gap-2.5 rounded-lg px-3 py-2 text-left transition-colors duration-150 cursor-pointer",
                    active
                      ? "text-foreground"
                      : "text-muted-foreground hover:text-foreground hover:bg-white/4",
                  )}
                >
                  {active && (
                    <motion.span
                      layoutId="nav-pill"
                      transition={{ type: "spring", stiffness: 480, damping: 38 }}
                      className="absolute inset-0 rounded-lg border border-primary/25 bg-linear-to-r from-primary/18 to-primary/6"
                    />
                  )}
                  <Icon
                    className={cn(
                      "relative z-10 size-4 transition-transform duration-150 group-hover:scale-110",
                      active && "text-orange-300",
                    )}
                  />
                  <span className="relative z-10 text-[13px] font-medium">{n.label}</span>
                  {active && (
                    <motion.span
                      layoutId="nav-dot"
                      transition={{ type: "spring", stiffness: 480, damping: 38 }}
                      className="relative z-10 ml-auto size-1.5 rounded-full bg-primary shadow-[0_0_8px] shadow-primary"
                    />
                  )}
                </button>
              );
            })}
          </nav>

          <div className="mt-auto px-4 pb-3.5 space-y-2.5">
            <div className="rounded-lg border border-border bg-white/[0.025] px-3 py-2">
              <div className="flex items-center gap-1.5 text-[10.5px] uppercase tracking-widest text-muted-foreground">
                <Bug className="size-3" /> hook engine
              </div>
              <div className="mt-1 flex items-center gap-1.5 text-[11.5px] text-emerald-300">
                <span className="size-1.5 rounded-full bg-emerald-400 animate-pulse-dot" />
                MinHook · 已装载
              </div>
            </div>
            <div className="text-[10.5px] text-muted-foreground/60 text-center">
              v0.1.0 · reverse + hook driven
            </div>
          </div>
        </aside>

        {/* ---------------- content ---------------- */}
        <main className="min-w-0 flex-1 overflow-hidden p-4 pl-5">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={tab}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
              className="h-full min-h-0"
            >
              {tab === "live" && <LiveLog onShowKb={showKb} />}
              {tab === "checkup" && <Checkup />}
              {tab === "kb" && <Kb highlight={kbHighlight} />}
              {tab === "shim" && <Shim />}
              {tab === "ensplogs" && <EnspLogs />}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
      <Toaster />
    </TooltipProvider>
  );
}
