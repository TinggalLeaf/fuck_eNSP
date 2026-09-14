import type { ReactNode } from "react";

/** Consistent page header: accent bar + title + description + right-side actions. */
export default function PageHeader({
  title,
  desc,
  children,
}: {
  title: ReactNode;
  desc?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="mb-3.5 flex items-end justify-between gap-4 flex-wrap">
      <div className="flex items-center gap-3 min-w-0">
        <span className="mb-0.5 h-6 w-1 rounded-full bg-linear-to-b from-primary to-primary/30 shadow-[0_0_10px] shadow-primary/50" />
        <div className="min-w-0">
          <h1 className="text-[17px] font-bold leading-tight tracking-tight">{title}</h1>
          {desc && (
            <p className="mt-0.5 text-xs text-muted-foreground truncate">{desc}</p>
          )}
        </div>
      </div>
      {children && <div className="flex items-center gap-2 flex-wrap">{children}</div>}
    </div>
  );
}
