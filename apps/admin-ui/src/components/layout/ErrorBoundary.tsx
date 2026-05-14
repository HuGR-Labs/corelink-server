"use client";

import * as React from "react";

interface State {
  error: Error | null;
}

export interface ErrorBoundaryProps {
  /** Override the fallback heading. */
  fallbackTitle?: string;
  /** Override the fallback body. */
  fallbackBody?: string;
  /** Report-issue URL or callback target. */
  reportUrl?: string;
  reportLabel?: string;
  children: React.ReactNode;
}

/**
 * React error boundary with a friendly accessible fallback.
 */
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    console.error("[ErrorBoundary]", error, info);
  }

  reset = () => this.setState({ error: null });

  render(): React.ReactNode {
    const {
      fallbackTitle = "Something went wrong.",
      fallbackBody = "We're sorry — an unexpected error occurred. Please try again or report the issue.",
      reportUrl = "/support/report",
      reportLabel = "Report issue",
      children,
    } = this.props;

    if (this.state.error) {
      return (
        <div role="alert" className="mx-auto max-w-md rounded-md border border-red-300 bg-red-50 p-4">
          <h2 className="text-lg font-semibold text-red-900">{fallbackTitle}</h2>
          <p className="mt-1 text-sm text-red-900">{fallbackBody}</p>
          <div className="mt-3 flex gap-3">
            <button
              type="button"
              onClick={this.reset}
              className="rounded bg-red-700 px-3 py-2 text-sm font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-900"
            >
              Try again
            </button>
            <a
              href={reportUrl}
              className="rounded border border-red-700 px-3 py-2 text-sm font-medium text-red-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-900"
            >
              {reportLabel}
            </a>
          </div>
        </div>
      );
    }

    return children;
  }
}
