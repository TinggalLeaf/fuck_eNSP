import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { Loader2 } from "lucide-react";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-[13px] font-medium transition-all duration-150 outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:pointer-events-none disabled:opacity-45 [&_svg]:size-3.5 [&_svg]:shrink-0 cursor-pointer select-none",
  {
    variants: {
      variant: {
        default:
          "bg-primary text-primary-foreground shadow-[0_0_18px_-4px] shadow-primary/50 hover:bg-orange-400 hover:shadow-primary/70 active:scale-[0.98]",
        secondary:
          "bg-secondary text-secondary-foreground border border-border hover:bg-white/10 active:scale-[0.98]",
        outline:
          "border border-border bg-transparent text-secondary-foreground hover:bg-white/6 hover:border-white/20 active:scale-[0.98]",
        ghost:
          "bg-transparent text-muted-foreground hover:bg-white/6 hover:text-foreground",
        destructive:
          "bg-destructive/15 text-red-300 border border-destructive/40 hover:bg-destructive/25 active:scale-[0.98]",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-8.5 px-3.5",
        sm: "h-7 px-2.5 text-xs",
        lg: "h-10 px-5 text-sm",
        icon: "h-8.5 w-8.5",
        "icon-sm": "h-7 w-7",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
  loading?: boolean;
}

function Button({ className, variant, size, asChild = false, loading = false, disabled, children, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      className={cn(buttonVariants({ variant, size, className }))}
      disabled={disabled || loading}
      {...props}
    >
      {loading && !asChild ? <Loader2 className="animate-spin" /> : null}
      {children}
    </Comp>
  );
}

export { Button, buttonVariants };
