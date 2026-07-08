import { forwardRef } from "react";
import type { SelectHTMLAttributes } from "react";

export type SelectProps = SelectHTMLAttributes<HTMLSelectElement>;

export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { className, children, ...rest },
  ref,
) {
  const classes = className ? "lin-select " + className : "lin-select";
  return (
    <select ref={ref} className={classes} {...rest}>
      {children}
    </select>
  );
});
