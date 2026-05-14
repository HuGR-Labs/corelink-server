"use client";

import * as React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export interface MarkdownViewProps {
  content: string;
  /** Optional className on the root wrapper. */
  className?: string;
}

/**
 * Renders untrusted markdown safely.
 * - `<script>` and raw HTML in the source are escaped (react-markdown does not
 *   render raw HTML unless `rehype-raw` is configured — which we deliberately do
 *   not include).
 * - GFM (tables, autolinks) enabled via remark-gfm.
 */
export function MarkdownView({ content, className }: MarkdownViewProps) {
  return (
    <div className={className ?? "prose prose-slate max-w-none"}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ node: _node, ...props }) => (
            <a {...props} className="underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700">
              {props.children}
            </a>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
