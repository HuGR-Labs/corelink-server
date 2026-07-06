"use client";

import { useState } from "react";
import { CodeBlock } from "./CodeBlock";

export interface SnippetTab {
  id: string;
  label: string;
  code: string;
  lang?: string;
}

export interface SnippetTabsProps {
  tabs: Array<SnippetTab>;
}

export function SnippetTabs({ tabs }: SnippetTabsProps) {
  const [activeId, setActiveId] = useState<string | undefined>(tabs[0]?.id);
  const active = tabs.find((t) => t.id === activeId) ?? tabs[0];

  return (
    <div className="lin-snippet">
      <div className="lin-snippet__tabs" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={tab.id === active?.id}
            className="lin-snippet__tab"
            data-active={tab.id === active?.id}
            onClick={() => setActiveId(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {active != null ? <CodeBlock code={active.code} lang={active.lang} /> : null}
    </div>
  );
}
