import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { Search, Info, CircleDot, Check } from "lucide-react";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";
import PageHeader from "./PageHeader";
import type { KbEntry } from "../types";
import { cn } from "../lib/utils";

interface Props {
  highlight: number | null;
}

export default function Kb({ highlight }: Props) {
  const [entries, setEntries] = useState<KbEntry[]>([]);
  const [q, setQ] = useState("");
  const flashRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<KbEntry[]>("error_kb").then(setEntries);
  }, []);

  useEffect(() => {
    if (highlight == null) return;
    const el = document.getElementById(`kb-${highlight}`);
    el?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [highlight, entries]);

  const filtered = useMemo(() => {
    if (!q) return entries;
    const f = q.toLowerCase();
    return entries.filter(
      (e) =>
        String(e.code).includes(f) ||
        e.title.toLowerCase().includes(f) ||
        e.meaning.toLowerCase().includes(f),
    );
  }, [entries, q]);

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <PageHeader title="错误码手册" desc="来自逆向客户端字符串表 + 社区经验；权威根因以「实时日志」服务端原始报文为准">
        <Badge variant="info">
          <Info /> {entries.length} 条收录
        </Badge>
      </PageHeader>

      <div className="relative w-72">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          placeholder="按错误码或关键字搜索…"
          className="pl-8"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>

      <div className="grid flex-1 auto-rows-min grid-cols-1 gap-3 overflow-y-auto pr-0.5 min-h-0 lg:grid-cols-2 2xl:grid-cols-3">
        {filtered.map((e, i) => (
          <motion.div
            key={e.code}
            id={`kb-${e.code}`}
            ref={highlight === e.code ? flashRef : undefined}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.25, delay: Math.min(i * 0.03, 0.3), ease: "easeOut" }}
            className={cn(
              "rounded-xl border bg-card/80 p-3.5 transition-shadow",
              highlight === e.code
                ? "border-primary/60 shadow-[0_0_0_2px_rgb(249_115_22/0.3),0_0_28px_-6px_rgb(249_115_22/0.4)]"
                : "border-border hover:border-white/15",
            )}
          >
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-sky-400/30 bg-sky-400/10 px-1.5 py-px font-mono text-xs font-bold text-sky-300">
                {e.code}
              </span>
              <span className="text-[13px] font-semibold">{e.title}</span>
            </div>
            <p className="mt-2 text-xs leading-relaxed text-muted-foreground">{e.meaning}</p>
            {e.causes.length > 0 && (
              <div className="mt-2.5">
                <div className="text-[11px] font-semibold tracking-wide text-orange-300">常见原因</div>
                <ul className="mt-1 space-y-1">
                  {e.causes.map((c, j) => (
                    <li key={j} className="flex gap-1.5 text-xs text-secondary-foreground">
                      <CircleDot className="mt-0.5 size-3 shrink-0 text-orange-400/70" />
                      {c}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {e.fixes.length > 0 && (
              <div className="mt-2.5">
                <div className="text-[11px] font-semibold tracking-wide text-emerald-300">解决办法</div>
                <ul className="mt-1 space-y-1">
                  {e.fixes.map((c, j) => (
                    <li key={j} className="flex gap-1.5 text-xs text-secondary-foreground">
                      <Check className="mt-0.5 size-3 shrink-0 text-emerald-400" />
                      {c}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </motion.div>
        ))}
        {filtered.length === 0 && (
          <div className="col-span-full py-16 text-center text-xs text-muted-foreground">
            没有匹配「{q}」的错误码
          </div>
        )}
      </div>
    </div>
  );
}
