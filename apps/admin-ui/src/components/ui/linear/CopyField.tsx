"use client";

import { useEffect, useRef, useState } from "react";

export interface CopyFieldProps {
  value: string;
  label?: string;
  redactAs?: string;
}

export function CopyField({ value, label, redactAs }: CopyFieldProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current != null) clearTimeout(timer.current);
    };
  }, []);

  const onCopy = () => {
    void navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      if (timer.current != null) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    });
  };

  const shown = redactAs ?? value;

  return (
    <div className="lin-copy">
      <span className="lin-copy__val" title={label}>
        {shown}
      </span>
      <button
        type="button"
        className="lin-copy__btn"
        onClick={onCopy}
        aria-label={label != null ? "Copy " + label : "Copy"}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
