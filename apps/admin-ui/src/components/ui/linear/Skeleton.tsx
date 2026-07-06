export interface SkeletonProps {
  rows?: number;
  width?: string;
}

export function Skeleton({ rows = 1, width }: SkeletonProps) {
  const count = Math.max(0, rows);
  return (
    <>
      {Array.from({ length: count }).map((_, i) => {
        const isLast = i === count - 1;
        const w = isLast ? width ?? "100%" : "100%";
        return <div key={i} className="lin-skel" style={{ width: w }} />;
      })}
    </>
  );
}
