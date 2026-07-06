import { forwardRef } from "react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost" | "danger";
  size?: "sm" | "md";
  loading?: boolean;
  iconLeft?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "primary",
    size = "md",
    loading,
    iconLeft,
    className,
    children,
    disabled,
    type,
    ...rest
  },
  ref,
) {
  const classes = ["lin-btn"];
  if (variant === "primary") classes.push("lin-btn--primary");
  else if (variant === "ghost") classes.push("lin-btn--ghost");
  else if (variant === "danger") classes.push("lin-btn--danger");
  if (size === "sm") classes.push("lin-btn--sm");
  if (className) classes.push(className);

  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={classes.join(" ")}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <span className="lin-btn__spin" aria-hidden="true" /> : null}
      {!loading && iconLeft != null ? iconLeft : null}
      {children}
    </button>
  );
});
