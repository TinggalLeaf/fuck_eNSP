import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { FileText, RefreshCw, AlertCircle, Files } from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import PageHeader from "./PageHeader";
import type { LogFile } from "../types";
import { fmtBytes } from "../types";
import { cn } from "../lib/utils";

export default function EnspLogs() {
  const [files, setFiles] = useState<LogFile[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState("");

  const refresh = () => invoke<LogFile[]>("ensp_logs").then(setFiles);
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, []);

  const open = async (path: string) => {
    setSel(path);
    setErr("");
    setLoading(true);
    try {
      setContent(await invoke("ensp_log_read", { path, tail_lines: 1000 }));
    } catch (e) {
      setErr(String(e));
      setContent("");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <PageHeader title="eNSP 日志" desc="eNSP 客户端与服务端落盘的日志文件（VBoxHardening.log 等）">
        <Button variant="secondary" size="sm" onClick={refresh}>
          <RefreshCw /> 刷新
        </Button>
        <Badge variant="outline">
          <Files /> {files.length} 个文件
        </Badge>
      </PageHeader>

      <div className="flex min-h-0 flex-1 gap-3">
        {/* file list */}
        <div className="flex w-72 shrink-0 flex-col overflow-hidden rounded-xl border border-border bg-card/70">
          <div className="border-b border-border px-3 py-2 text-[11px] uppercase tracking-widest text-muted-foreground">
            日志文件
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {files.length === 0 ? (
              <div className="flex flex-col items-center gap-2 px-4 py-14 text-center text-xs text-muted-foreground">
                <FileText className="size-6 opacity-30" />
                未发现日志，先用「实时日志」页启动一次 eNSP
              </div>
            ) : (
              files.map((f) => {
                const name = f.path.split("\\").pop() ?? f.path;
                const active = sel === f.path;
                return (
                  <button
                    key={f.path}
                    onClick={() => open(f.path)}
                    title={f.path}
                    className={cn(
                      "relative flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                      active
                        ? "bg-primary/12 text-foreground"
                        : "text-muted-foreground hover:bg-white/5 hover:text-foreground",
                    )}
                  >
                    {active && (
                      <motion.span
                        layoutId="ensplog-active"
                        transition={{ type: "spring", stiffness: 500, damping: 40 }}
                        className="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-full bg-primary"
                      />
                    )}
                    <FileText className={cn("size-3.5 shrink-0", active && "text-orange-300")} />
                    <span className="min-w-0 flex-1 truncate font-mono text-xs">{name}</span>
                    <span className="shrink-0 text-[10px] text-muted-foreground/70">{fmtBytes(f.size)}</span>
                  </button>
                );
              })
            )}
          </div>
        </div>

        {/* viewer */}
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <AnimatePresence>
            {err && (
              <motion.div
                initial={{ opacity: 0, y: -6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                className="flex items-center gap-2 rounded-lg border border-red-400/30 bg-red-400/10 px-3 py-2 text-xs text-red-300"
              >
                <AlertCircle className="size-3.5 shrink-0" /> {err}
              </motion.div>
            )}
          </AnimatePresence>
          <div className="min-h-0 flex-1 overflow-auto rounded-xl border border-border bg-black/45 p-3.5 font-mono text-[12px] leading-relaxed whitespace-pre-wrap text-[#c9d4e3]">
            {loading ? (
              <span className="text-muted-foreground">读取中…</span>
            ) : (
              content || (
                <span className="text-muted-foreground/70">
                  {sel ? "（空）" : "从左侧选择文件查看内容"}
                </span>
              )
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
