"use client";

import * as React from "react";
import { ALL_STEPS, type OnboardingStep } from "@/lib/onboarding-state";

export interface WizardProps {
  current: OnboardingStep;
  children: React.ReactNode;
  /** Optional override for nav handlers; pages use this to gate continue. */
  onBack?: () => void;
  onNext?: () => void;
  canBack?: boolean;
  canNext?: boolean;
  /** Labels — passed in by the page so they can be translated. */
  labels?: {
    back?: string;
    next?: string;
    stepFormat?: (current: number, total: number) => string;
  };
}

export function stepIndex(step: OnboardingStep): number {
  return ALL_STEPS.indexOf(step);
}

export function Wizard(props: WizardProps): React.ReactElement {
  const { current, children, onBack, onNext, canBack, canNext, labels } = props;
  const idx = stepIndex(current);
  const total = ALL_STEPS.length;
  const display =
    labels?.stepFormat?.(idx + 1, total) ?? `Step ${idx + 1} of ${total}`;

  return (
    <div className="wizard" data-testid="wizard">
      <header className="wizard-header">
        <ol className="wizard-steps" aria-label="Onboarding steps">
          {ALL_STEPS.map((s, i) => (
            <li
              key={s}
              data-step={s}
              data-state={i < idx ? "done" : i === idx ? "current" : "pending"}
              aria-current={i === idx ? "step" : undefined}
            >
              {s}
            </li>
          ))}
        </ol>
        <p className="wizard-progress" data-testid="wizard-progress">
          {display}
        </p>
      </header>

      <section className="wizard-body">{children}</section>

      <footer className="wizard-footer">
        {onBack ? (
          <button
            type="button"
            onClick={onBack}
            disabled={canBack === false || idx === 0}
            data-testid="wizard-back"
          >
            {labels?.back ?? "Back"}
          </button>
        ) : null}
        {onNext ? (
          <button
            type="button"
            onClick={onNext}
            disabled={canNext === false}
            data-testid="wizard-next"
          >
            {labels?.next ?? "Next"}
          </button>
        ) : null}
      </footer>
    </div>
  );
}
