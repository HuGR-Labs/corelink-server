"use client";

import { useEffect, useRef, useState } from "react";

export interface CodeBlockProps {
  code: string;
  lang?: string;
}

export function CodeBlock({ code, lang }: CodeBlockProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current != null) clearTimeout(timer.current);
    };
  }, []);

  const onCopy = () => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(true);
      if (timer.current != null) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div className="lin-code">
      <button
        type="button"
        className="lin-code__copy lin-btn lin-btn--ghost lin-btn--sm"
        onClick={onCopy}
        aria-label="Copy code"
      >
        {copied ? "Copied" : "Copy"}
      </button>
      <pre>
        <code data-lang={lang}>{code}</code>
      </pre>
    </div>
  );
}
