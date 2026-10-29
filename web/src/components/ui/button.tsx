import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

// Picked: Ink (frontend-spec 3.3). Destructive is for the final confirm only.
export const buttonStyles = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md border font-medium transition-colors duration-[var(--duration-fast)] disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "border-transparent bg-primary text-primary-text hover:opacity-90",
        secondary: "border-border bg-surface text-text hover:bg-sunken",
        ghost: "border-transparent bg-transparent text-text hover:bg-sunken",
        destructive: "border-transparent bg-danger text-danger-on hover:opacity-90",
        link: "h-auto border-transparent bg-transparent p-0 text-text underline-offset-4 hover:underline",
      },
      size: {
        sm: "h-8 px-3 text-sm",
        md: "h-9 px-4 text-base",
        lg: "h-11 px-5 text-md",
        icon: "size-9 p-0",
        iconSm: "size-8 p-0",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonStyles> {
  /** Render the child (an `<a>`, a router `<Link>`) with button styles. */
  asChild?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant, size, asChild, type, ...props },
  ref,
) {
  const Comp = asChild ? Slot.Root : "button";
  return (
    <Comp
      ref={ref}
      type={asChild ? undefined : (type ?? "button")}
      className={cn(buttonStyles({ variant, size }), className)}
      {...props}
    />
  );
});
