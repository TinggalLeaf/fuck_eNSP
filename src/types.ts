export interface HookEntry {
  ts: number;
  pid: number;
  proc: string;
  cat: string;
  api: string;
  peer?: string;
  dir?: string;
  len?: number;
  ok?: boolean;
  child_pid?: number;
  app?: string;
  cmd?: string;
  enc?: string;
  data?: string;
  level: string;
  text: string;
  err_code?: number | null;
}

export interface InjectResult {
  pid: number;
  name: string;
  ok: boolean;
  message: string;
}

export interface DiscoverInfo {
  hook_dir: string;
  ensp_processes: { pid: number; name: string }[];
  hooked_pids: number[];
  watching: boolean;
}

export interface CheckItem {
  id: string;
  title: string;
  status: "pass" | "warn" | "fail" | "info";
  detail: string;
  fix?: string;
}

export interface KbEntry {
  code: number;
  title: string;
  meaning: string;
  causes: string[];
  fixes: string[];
}

export interface LogFile {
  path: string;
  size: number;
  modified: string;
}

export function fmtTime(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${String(
    d.getMilliseconds(),
  ).padStart(3, "0")}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
