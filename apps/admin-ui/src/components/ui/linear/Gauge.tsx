export interface GaugeProps {
  label: string;
  value: number;
  max: number;
  unit?: string;
  hint?: string;
  warnAt?: number;
  dangerAt?: number;
}

export function Gauge({
  label,
  value,
  max,
  unit,
  hint,
  warnAt = 0.7,
  dangerAt = 0.9,
}: GaugeProps) {
  const pct = max > 0 ? value / max : 0;
  const clamped = Math.max(0, Math.min(1, pct));

  const fillClasses = ["lin-gauge__fill"];
  if (pct >= dangerAt) fillClasses.push("lin-gauge__fill--danger");
  else if (pct >= warnAt) fillClasses.push("lin-gauge__fill--warn");

  const suffix = unit != null ? " " + unit : "";

  return (
    <div>
      <div className="lin-gauge__top">
        <span className="lin-gauge__label">
          {label} {value}/{max}
          {suffix}
        </span>
        <span className="lin-gauge__pct">{Math.round(pct * 100)}%</span>
      </div>
      <div className="lin-gauge__track">
        <div
          className={fillClasses.join(" ")}
          style={{ width: clamped * 100 + "%" }}
        />
      </div>
      {hint != null ? <div className="lin-gauge__hint">{hint}</div> : null}
    </div>
  );
}
