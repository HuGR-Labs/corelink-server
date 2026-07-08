export interface InlineErrorProps {
  error: unknown;
  onRetry?: () => void;
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

export function InlineError({ error, onRetry }: InlineErrorProps) {
  return (
    <div className="lin-err" role="alert">
      <div className="lin-err__msg">
        <strong>Something went wrong</strong>
        {" — "}
        {messageOf(error)}
      </div>
      {onRetry != null ? (
        <button
          type="button"
          className="lin-btn lin-btn--ghost lin-btn--sm"
          onClick={onRetry}
        >
          Retry
        </button>
      ) : null}
    </div>
  );
}
