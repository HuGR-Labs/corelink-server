import type { ReactNode } from "react";
import { HelpPopover } from "./HelpPopover";

export interface FieldProps {
  label: string;
  htmlFor?: string;
  help?: string;
  children: ReactNode;
}

export function Field({ label, htmlFor, help, children }: FieldProps) {
  return (
    <div>
      <label className="lin-label" htmlFor={htmlFor}>
        {label}
        {help != null ? <HelpPopover label={label}>{help}</HelpPopover> : null}
      </label>
      {children}
    </div>
  );
}
