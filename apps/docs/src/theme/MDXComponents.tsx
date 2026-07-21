/**
 * MDXComponents — wrap swizzle of the classic theme's MDX component map.
 *
 * Why: GitHub-flavoured-Markdown task lists (`- [ ]` / `- [x]`) are rendered
 * by remark-gfm as a disabled `<input type="checkbox">` with NO programmatic
 * label. axe flags every such input as a `critical` `label` violation
 * (WCAG 4.1.2 Name, Role, Value) regardless of the `disabled` state — the
 * input must expose an accessible name. Several docs pages (e.g. the
 * `/security` and `/compliance` review checklists) use task lists, so this is
 * a systemic issue best fixed once at the theme layer rather than by editing
 * each `.mdx` file.
 *
 * Fix: override the MDX `input` element so checkbox inputs receive an
 * `aria-label` reflecting their checked state. The visible list-item text
 * still follows in the reading order, so a screen-reader user hears e.g.
 * "Checklist item, not done — Security Lead — confirm …". All other inputs
 * (none currently authored in MDX; the React `NewsletterSignup`/search inputs
 * are JSX components and do NOT pass through this map) are rendered unchanged.
 */

import { type ComponentProps, type ReactElement } from "react";
import MDXComponents from "@theme-original/MDXComponents";

function MDXInput(props: ComponentProps<"input">): ReactElement {
  if (props.type === "checkbox") {
    return (
      <input
        {...props}
        aria-label={
          props.checked ? "Checklist item, done" : "Checklist item, not done"
        }
      />
    );
  }
  return <input {...props} />;
}

export default {
  ...MDXComponents,
  input: MDXInput,
};
