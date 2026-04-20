import * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "flex field-sizing-content min-h-16 w-full rounded-lg border border-input bg-white/50 px-2.5 py-2 text-base shadow-[inset_0_1px_0_oklch(1_0_0_/_72%)] backdrop-blur-xl transition-all duration-200 outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:bg-white/64 focus-visible:ring-3 focus-visible:ring-ring/28 disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 md:text-sm dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
