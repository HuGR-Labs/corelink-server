import { forwardRef } from "react";
import type { TextareaHTMLAttributes } from "react";

export type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement>;

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  function Textarea({ className, ...rest }, ref) {
    const classes = className ? "lin-textarea " + className : "lin-textarea";
    return <textarea ref={ref} className={classes} {...rest} />;
  },
);
